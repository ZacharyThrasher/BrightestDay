#!/usr/bin/env python3
"""Build the compact static-vector artifact loaded by src/semantic/embed.mjs.

    python3 tools/semantic/build_artifact.py                       # numberbatch, 100k words, SVD->200d (default)
    python3 tools/semantic/build_artifact.py --source spacy-lg     # MIT-licensed alternative
    python3 tools/semantic/build_artifact.py --source glove        # PDDL-licensed alternative
    options: --top-words N  --no-compounds  --dims K (uncentred truncated SVD)  --out name.bin

Prereqs: `node tools/semantic/download.mjs --group embed`, then `python3 tools/semantic/bench_embed.py
<source>` once (it caches the parsed table as models/semantic/cache/*.npy); the numberbatch/spacy
parsers are reused from bench_embed.py so this script also works from a clean cache.

Vocabulary selection (no dependence on the test set):
  1. the N most frequent alphabetic words (frequency order = GloVe 6B vocab order) present in the source
  2. single-word WordNet noun lemmas that descend from object.n.01 (rare object names: trebuchet, buckler...)
  3. multi-word WordNet noun lemmas descending from object.n.01 (pogo_stick, wrecking_ball...) -- only
     for sources that have phrase vectors (numberbatch)
  4. every alias in tools/semantic/concepts.json (so the stand-in lexicon is always covered)
  5. terms listed in tools/semantic/extra_terms.txt (game-specific words; --extra FILE)

File format "BDSV" v1 (little endian):
  0  4 bytes  magic 'BDSV'
  4  u32      header JSON byte length H
  8  H bytes  UTF-8 JSON header {version,dims,count,quant,source,license,...}
  .. pad to 4-byte boundary
  .. u32      vocab byte length V, then V bytes UTF-8 terms joined by '\\n' (row order; multiword terms use '_')
  .. pad to 4
  .. float32[count] per-row scale  (row_unit = int8_row * scale)
  .. int8[count*dims] row-major, rows L2-normalised before quantisation
"""
import argparse, hashlib, json, os, struct, sys
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import bench_embed as B  # noqa: E402

OUT_DIR = os.path.join(B.REPO, 'models', 'semantic')


def load_source(name):
    if name == 'numberbatch':
        idx, m = B.load_text_vectors('numberbatch_en_1908', os.path.join(B.RAW, 'numberbatch-en-19.08.txt.gz'))
        return idx, m, dict(source='ConceptNet Numberbatch 19.08 (English)', license='CC BY-SA 4.0',
                            attribution='ConceptNet Numberbatch by Luminoso / Robyn Speer et al., https://github.com/commonsense/conceptnet-numberbatch',
                            phrases=True, url='https://conceptnet.s3.amazonaws.com/downloads/2019/numberbatch/numberbatch-en-19.08.txt.gz')
    if name == 'spacy-lg':
        idx, m = B.load_spacy('en_core_web_lg')
        return idx, m, dict(source='spaCy en_core_web_lg 3.8.0 vectors', license='MIT',
                            attribution='Explosion AI spaCy en_core_web_lg', phrases=False,
                            url='https://github.com/explosion/spacy-models/releases/download/en_core_web_lg-3.8.0/en_core_web_lg-3.8.0-py3-none-any.whl')
    if name == 'glove':
        idx, m = B.load_text_vectors('glove300', os.path.join(B.RAW, 'glove-wiki-gigaword-300.gz'))
        return idx, m, dict(source='GloVe 6B 300d (Wikipedia 2014 + Gigaword 5)', license='PDDL 1.0',
                            attribution='GloVe, Pennington, Socher, Manning (Stanford NLP)', phrases=False,
                            url='https://github.com/RaRe-Technologies/gensim-data/releases/download/glove-wiki-gigaword-300/glove-wiki-gigaword-300.gz')
    raise SystemExit('unknown source ' + name)


def wordnet_objects():
    import nltk
    nltk.data.path.insert(0, os.path.join(B.RAW, 'nltk_data'))
    from nltk.corpus import wordnet as wn
    obj = wn.synset('object.n.01')
    singles, compounds = set(), set()
    for s in wn.all_synsets('n'):
        if not any(obj in p for p in s.hypernym_paths()):
            continue
        for l in s.lemma_names():
            l = l.lower()
            if not all(c.isalpha() or c in "_-'" for c in l):
                continue
            (compounds if '_' in l or '-' in l else singles).add(l.replace('-', '_') if '_' in l else l)
    return singles, compounds


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--source', default='numberbatch', choices=['numberbatch', 'spacy-lg', 'glove'])
    ap.add_argument('--top-words', type=int, default=None, help='default: 100000 for numberbatch, 40000 otherwise')
    ap.add_argument('--no-compounds', action='store_true')
    ap.add_argument('--dims', type=int, default=None, help='truncated-SVD reduce to this many dims (default: 200 for numberbatch, keep otherwise)')
    ap.add_argument('--extra', default=os.path.join(HERE, 'extra_terms.txt'), help='file of extra terms to force in')
    ap.add_argument('--out', default=None)
    a = ap.parse_args()
    if a.top_words is None:
        a.top_words = 100000 if a.source == 'numberbatch' else 40000
    if a.dims is None:
        a.dims = 200 if a.source == 'numberbatch' else 0

    idx, mat, meta = load_source(a.source)
    lower_map = {}
    if a.source == 'spacy-lg':  # spaCy keys are case-sensitive; prefer the lowercase key
        for w, i in idx.items():
            lw = w.lower()
            if lw == w or lw not in lower_map:
                lower_map[lw] = i
    else:
        lower_map = idx
    glove_order = open(os.path.join(B.CACHE, 'glove300.vocab.txt'), encoding='utf8').read().split('\n') \
        if os.path.exists(os.path.join(B.CACHE, 'glove300.vocab.txt')) else None
    if glove_order is None:
        B.load_text_vectors('glove300', os.path.join(B.RAW, 'glove-wiki-gigaword-300.gz'))
        glove_order = open(os.path.join(B.CACHE, 'glove300.vocab.txt'), encoding='utf8').read().split('\n')

    chosen, seen = [], set()

    def add(term):
        if term in seen or term not in lower_map:
            return False
        seen.add(term); chosen.append(term); return True

    n_top = 0
    for w in glove_order:
        if n_top >= a.top_words:
            break
        if w.isalpha() and add(w):
            n_top += 1
    singles, compounds = wordnet_objects()
    n_wn1 = sum(add(w) for w in sorted(singles))
    n_wn2 = 0
    if meta['phrases'] and not a.no_compounds:
        n_wn2 = sum(add(w) for w in sorted(compounds))
    n_alias = 0
    for c in B.CONCEPTS:
        for al in c[1]:
            t = al.lower().replace(' ', '_') if meta['phrases'] else None
            if t and add(t):
                n_alias += 1
            for tok in B.tokens(al):
                n_alias += add(tok)

    n_extra = 0
    if a.extra and os.path.exists(a.extra):
        for line in open(a.extra, encoding='utf8'):
            t = line.strip().lower()
            if t and not t.startswith('#'):
                n_extra += add(t.replace(' ', '_').replace('-', '_') if meta['phrases'] else t) or add(t)

    rows = np.vstack([np.asarray(mat[lower_map[t]], dtype=np.float32) for t in chosen])
    rows = B.norm_rows(rows)
    if a.dims and a.dims < rows.shape[1]:
        # uncentred truncated SVD: best rank-k approximation of all pairwise dot products
        _, sv, vt = np.linalg.svd(rows, full_matrices=False)
        rows = B.norm_rows(rows @ vt[:a.dims].T)
        print(f'svd: kept {a.dims} dims, energy {float((sv[:a.dims]**2).sum()/(sv**2).sum()):.3f}')
    scale = np.abs(rows).max(axis=1) / 127.0
    scale[scale == 0] = 1
    q = np.clip(np.rint(rows / scale[:, None]), -127, 127).astype(np.int8)
    err = np.abs(q * scale[:, None] - rows).max()

    dims, count = rows.shape[1], rows.shape[0]
    header = dict(version=1, dims=int(dims), count=int(count), quant='int8-rowscale', normalized=True,
                  svdDims=int(a.dims) if a.dims else None,
                  phraseJoiner='_' if meta['phrases'] else None, source=meta['source'], license=meta['license'],
                  attribution=meta['attribution'], sourceUrl=meta['url'],
                  selection=dict(topWords=n_top, wordnetObjectSingles=n_wn1, wordnetObjectCompounds=n_wn2, conceptAliases=n_alias,
                                 extraTerms=n_extra),
                  maxAbsQuantError=float(err))
    hb = json.dumps(header).encode('utf8')
    vb = '\n'.join(chosen).encode('utf8')
    name = a.out or {'numberbatch': 'bd-semantic-nb19-int8.bin', 'spacy-lg': 'bd-semantic-spacylg-int8.bin',
                     'glove': 'bd-semantic-glove-int8.bin'}[a.source]
    path = os.path.join(OUT_DIR, name)
    pad = lambda n: b'\0' * ((4 - n % 4) % 4)
    with open(path, 'wb') as f:
        f.write(b'BDSV'); f.write(struct.pack('<I', len(hb))); f.write(hb); f.write(pad(8 + len(hb)))
        pos = f.tell()
        f.write(struct.pack('<I', len(vb))); f.write(vb); f.write(pad(pos + 4 + len(vb)))
        f.write(scale.astype('<f4').tobytes()); f.write(q.tobytes())
    data = open(path, 'rb').read()
    sha = hashlib.sha256(data).hexdigest()
    lock = json.load(open(os.path.join(HERE, 'sources.lock.json')))
    src_file = os.path.basename(meta['url'])
    manifest = dict(file=name, url=None, urlNote='not hosted yet; rebuild with `builder` or host as a release asset and fill in url',
                    sha256=sha, size=len(data), header=header,
                    builtFrom=dict(url=meta['url'], sha256=lock.get(src_file, {}).get('sha256')),
                    builder='tools/semantic/build_artifact.py --source ' + a.source)
    mpath = path.replace('.bin', '.manifest.json')
    json.dump(manifest, open(mpath, 'w'), indent=2)
    print(f'wrote {path} ({len(data)/1e6:.1f} MB, {count} terms x {dims}d) sha256={sha}')
    print(json.dumps(header['selection']), 'maxAbsQuantError=%.4f' % err)


if __name__ == '__main__':
    main()
