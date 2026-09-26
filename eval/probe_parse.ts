/** Dev probe: print the parser's clause structure.  npx tsx eval/probe_parse.ts "utterance" ... */
import { parse } from '../src/lang/parser.js';

for (const u of process.argv.slice(2)) {
  const p = parse(u);
  console.log(u, '=>', p.normalized);
  for (const c of p.clauses) {
    console.log(JSON.stringify({
      verb: c.verb, vclass: c.vclass, causative: c.causative,
      objects: c.objects.map((o) => ({ t: o.tokens, c: o.concept?.id, ref: o.ref, count: o.count, unk: o.unknownHead, addons: o.addons.map((a) => a.addon.id), rel: o.rel.map((r) => `${r.verb}:${r.raw.join(' ')}`) })),
      preps: c.preps.map((q) => [q.prep, q.np.tokens, q.np.concept?.id, q.np.addons.map((a) => a.addon.id), q.np.rel.map((r) => r.raw.join(' '))]),
    }));
  }
}
