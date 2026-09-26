#!/usr/bin/env python3
"""Offline comparison of System-1 "unknown phrase -> nearest known concept" methods.

    python3 tools/semantic/bench_embed.py                 # all methods
    python3 tools/semantic/bench_embed.py numberbatch glove
    python3 tools/semantic/bench_embed.py --json out.json

Inputs: tools/semantic/concepts.json, tools/semantic/testset.json and the raw files fetched by
download.mjs into models/semantic/raw/. Parsed vector tables are cached as .npy under
models/semantic/cache/ (first run of GloVe / Numberbatch takes ~1-2 min each).

Scoring: concept score = max cosine over its aliases; top-1 / top-3 = best / any-of-top-3 concept is
in the item's `accept` list. Latency = wall time per query (embed phrase + score ~150 alias vectors)
in this Python process on this 4-core container; the shipped JS module is timed separately
(src/semantic/embed.test.mjs).
"""
import gzip, json, os, re, sys, time
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..'))
RAW = os.path.join(REPO, 'models', 'semantic', 'raw')
CACHE = os.path.join(REPO, 'models', 'semantic', 'cache')
os.makedirs(CACHE, exist_ok=True)

CONCEPTS = [(c[0], c[1:]) for c in json.load(open(os.path.join(HERE, 'concepts.json')))['concepts']]
ITEMS = json.load(open(os.path.join(HERE, 'testset.json')))['items']


def tokens(text):
    return [t for t in re.split(r"[^a-z0-9']+", text.lower()) if t]


def norm_rows(m):
    n = np.linalg.norm(m, axis=1, keepdims=True)
    n[n == 0] = 1
    return m / n


# ---------------------------------------------------------------- static tables
def load_text_vectors(name, path, skip_header=True, max_rows=None):
    npy, voc = os.path.join(CACHE, name + '.npy'), os.path.join(CACHE, name + '.vocab.txt')
    if os.path.exists(npy):
        words = open(voc, encoding='utf8').read().split('\n')
        return {w: i for i, w in enumerate(words)}, np.load(npy, mmap_mode='r')
    words, rows = [], []
    with gzip.open(path, 'rt', encoding='utf8') as f:
        if skip_header:
            f.readline()
        for line in f:
            p = line.rstrip('\n').split(' ')
            if len(p) < 50:
                continue
            words.append(p[0]); rows.append(np.asarray(p[1:], dtype=np.float32))
            if max_rows and len(words) >= max_rows:
                break
    m = np.vstack(rows)
    np.save(npy, m)
    open(voc, 'w', encoding='utf8').write('\n'.join(words))
    return {w: i for i, w in enumerate(words)}, m


def load_spacy(name):
    import spacy
    npy, voc = os.path.join(CACHE, name + '.npy'), os.path.join(CACHE, name + '.vocab.txt')
    if os.path.exists(npy):
        words = open(voc, encoding='utf8').read().split('\n')
        return {w: i for i, w in enumerate(words)}, np.load(npy, mmap_mode='r')
    nlp = spacy.load(name, exclude=['tagger', 'parser', 'ner', 'lemmatizer', 'attribute_ruler', 'senter'])
    vec = nlp.vocab.vectors
    words, rows = [], []
    data = np.asarray(vec.data)
    for key, row in vec.key2row.items():
        s = nlp.vocab.strings[key]
        words.append(s); rows.append(row)
    m = data[np.asarray(rows)]
    np.save(npy, m.astype(np.float32))
    open(voc, 'w', encoding='utf8').write('\n'.join(words))
    return {w: i for i, w in enumerate(words)}, np.load(npy, mmap_mode='r')


class Static:
    """Mean of word vectors; optional whole-phrase lookup (joined with `joiner`) and head-word weight."""

    def __init__(self, index, mat, joiner=None, head_weight=1.0, lower=True):
        self.index, self.mat, self.joiner, self.hw, self.lower = index, mat, joiner, head_weight, lower

    def row(self, w):
        i = self.index.get(w)
        if i is None and not self.lower:
            i = self.index.get(w.lower())
        return None if i is None else np.asarray(self.mat[i], dtype=np.float32)

    def embed(self, text):
        t = text.strip().lower() if self.lower else text.strip()
        if self.joiner is not None:
            for cand in (re.sub(r'[\s\-]+', self.joiner, t), t):
                v = self.row(cand)
                if v is not None:
                    return v
        v = self.row(t)
        if v is not None:
            return v
        toks = tokens(t)
        vs, ws = [], []
        for k, tok in enumerate(toks):
            v = self.row(tok)
            if v is not None:
                v = v / (np.linalg.norm(v) or 1)
                vs.append(v); ws.append(self.hw if k == len(toks) - 1 else 1.0)
        if not vs:
            return None
        return np.average(np.vstack(vs), axis=0, weights=ws)


# ---------------------------------------------------------------- transformer encoders (ONNX)
class Onnx:
    def __init__(self, folder, pooling, prefix=''):
        import onnxruntime as ort
        from tokenizers import Tokenizer
        so = ort.SessionOptions(); so.intra_op_num_threads = 4
        f = [x for x in os.listdir(folder) if x.endswith('.onnx') and not x.startswith('._')][0]
        self.sess = ort.InferenceSession(os.path.join(folder, f), so, providers=['CPUExecutionProvider'])
        self.tok = Tokenizer.from_file(os.path.join(folder, 'tokenizer.json'))
        self.tok.no_padding(); self.tok.no_truncation()
        self.pooling, self.prefix = pooling, prefix
        self.inputs = [i.name for i in self.sess.get_inputs()]

    def embed(self, text):
        e = self.tok.encode(self.prefix + text)
        ids = np.asarray([e.ids], dtype=np.int64)
        feed = {'input_ids': ids, 'attention_mask': np.ones_like(ids)}
        if 'token_type_ids' in self.inputs:
            feed['token_type_ids'] = np.zeros_like(ids)
        out = self.sess.run(None, feed)[0][0]
        return out[0] if self.pooling == 'cls' else out.mean(axis=0)


# ---------------------------------------------------------------- WordNet
class WordNetSim:
    def __init__(self):
        import nltk
        nltk.data.path.insert(0, os.path.join(RAW, 'nltk_data'))
        from nltk.corpus import wordnet as wn
        self.wn = wn
        self.csyn = {cid: [s for a in al for s in wn.synsets(a.replace(' ', '_'), pos='n')] for cid, al in CONCEPTS}

    def synsets(self, text):
        t = text.lower().strip()
        s = self.wn.synsets(re.sub(r'[\s\-]+', '_', t), pos='n') or self.wn.synsets(t, pos='n')
        if not s and tokens(t):
            s = self.wn.synsets(tokens(t)[-1], pos='n')  # head noun fallback
        return s

    def rank(self, text):
        qs = self.synsets(text)
        if not qs:
            return None
        out = []
        for cid, cs in self.csyn.items():
            best = 0.0
            for a in qs[:4]:
                for b in cs[:6]:
                    best = max(best, a.wup_similarity(b) or 0.0)
            out.append((cid, best))
        return sorted(out, key=lambda x: -x[1])


# ---------------------------------------------------------------- evaluation
def evaluate(name, embed_fn=None, rank_fn=None):
    t0 = time.perf_counter()
    if rank_fn is None:
        cvecs, cids = [], []
        for cid, al in CONCEPTS:
            for a in al:
                v = embed_fn(a)
                if v is not None:
                    cvecs.append(v); cids.append(cid)
        C = norm_rows(np.vstack(cvecs).astype(np.float32))
        cids = np.asarray(cids)
        missing_concepts = sorted({c for c, _ in CONCEPTS} - set(cids.tolist()))

        def rank_fn(text):
            v = embed_fn(text)
            if v is None:
                return None
            s = C @ (v / (np.linalg.norm(v) or 1)).astype(np.float32)
            best = {}
            for cid, sc in zip(cids, s):
                if sc > best.get(cid, -9):
                    best[cid] = float(sc)
            return sorted(best.items(), key=lambda x: -x[1])
    else:
        missing_concepts = []
    setup = time.perf_counter() - t0
    top1 = top3 = oov = 0
    lat, rows = [], []
    for phrase, accept in ITEMS:
        t = time.perf_counter()
        r = rank_fn(phrase)
        lat.append(time.perf_counter() - t)
        if not r:
            oov += 1; rows.append((phrase, None)); continue
        ids = [c for c, _ in r[:3]]
        top1 += ids[0] in accept
        top3 += any(i in accept for i in ids)
        rows.append((phrase, [(c, round(s, 3)) for c, s in r[:3]]))
    n = len(ITEMS)
    lat_ms = np.asarray(lat) * 1000
    res = dict(method=name, n=n, top1=top1 / n, top3=top3 / n, oov=oov, missing_concepts=missing_concepts,
               lat_ms_mean=float(lat_ms.mean()), lat_ms_p95=float(np.percentile(lat_ms, 95)), setup_s=setup,
               details=rows)
    print(f"{name:34s} top1={top1/n:.3f} top3={top3/n:.3f} oov={oov:3d} lat_mean={lat_ms.mean():7.2f}ms "
          f"p95={np.percentile(lat_ms,95):7.2f}ms missingConcepts={len(missing_concepts)}", flush=True)
    return res


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    out_json = sys.argv[sys.argv.index('--json') + 1] if '--json' in sys.argv else None
    if out_json:
        args = [a for a in args if a != out_json]
    want = lambda k: not args or any(a in k for a in args)
    results = []
    X = os.path.join(RAW, 'x')
    if want('spacy-md'):
        i, m = load_spacy('en_core_web_md')
        results.append(evaluate('spacy-md (mean)', Static(i, m, lower=False).embed))
    if want('spacy-lg'):
        i, m = load_spacy('en_core_web_lg')
        results.append(evaluate('spacy-lg (mean)', Static(i, m, lower=False).embed))
        results.append(evaluate('spacy-lg (mean, head x2)', Static(i, m, lower=False, head_weight=2).embed))
    if want('glove'):
        i, m = load_text_vectors('glove300', os.path.join(RAW, 'glove-wiki-gigaword-300.gz'))
        results.append(evaluate('glove-300 (mean)', Static(i, m).embed))
        results.append(evaluate('glove-300 (mean, head x2)', Static(i, m, head_weight=2).embed))
    if want('numberbatch'):
        i, m = load_text_vectors('numberbatch_en_1908', os.path.join(RAW, 'numberbatch-en-19.08.txt.gz'))
        results.append(evaluate('numberbatch-19.08 (mean only)', Static(i, m).embed))
        results.append(evaluate('numberbatch-19.08 (phrase+mean)', Static(i, m, joiner='_').embed))
        results.append(evaluate('numberbatch-19.08 (phrase+head x2)', Static(i, m, joiner='_', head_weight=2).embed))
    if want('wordnet'):
        w = WordNetSim()
        results.append(evaluate('wordnet wup (max synsets)', rank_fn=w.rank))
    if want('minilm'):
        o = Onnx(os.path.join(X, 'fast-all-MiniLM-L6-v2'), 'mean')
        results.append(evaluate('all-MiniLM-L6-v2 onnx', o.embed))
    if want('bge'):
        o = Onnx(os.path.join(X, 'fast-bge-small-en-v1.5'), 'cls')
        results.append(evaluate('bge-small-en-v1.5 onnx', o.embed))
        o2 = Onnx(os.path.join(X, 'fast-bge-small-en-v1.5'), 'cls', prefix='a ')
        results.append(evaluate('bge-small-en-v1.5 onnx ("a X")', o2.embed))
    if want('use'):
        p = os.path.join(CACHE, 'use_lite_embeddings.json')
        if os.path.exists(p):
            E = json.load(open(p))
            tab = {k: np.asarray(v, dtype=np.float32) for k, v in E['vectors'].items()}
            r = evaluate('USE-lite tfjs (precomputed)', lambda t: tab.get(t))
            r['lat_ms_mean'], r['lat_ms_p95'] = E['lat_ms_mean'], E['lat_ms_p95']
            print(f"   (USE latency measured in node: mean={E['lat_ms_mean']:.2f}ms p95={E['lat_ms_p95']:.2f}ms, backend={E['backend']})")
            results.append(r)
        else:
            print('USE: run `node tools/semantic/bench_use.mjs` first')
    for extra in ('artifact',):
        if want(extra) and args:
            pass
    if out_json:
        json.dump(results, open(out_json, 'w'), indent=1)


if __name__ == '__main__':
    main()
