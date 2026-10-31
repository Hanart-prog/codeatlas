// Compares CodeAtlas edges with the TypeScript compiler's module resolution.
// Usage: node verify_imports.cjs <typescript module> <repository> <graph.json>
const ts = require(process.argv[2]); const path = require("path"); const fs = require("fs");
const root = fs.realpathSync(process.argv[3]); const graph = JSON.parse(fs.readFileSync(process.argv[4], "utf8"));
const exclude = /(^|\/)(node_modules|vendor|dist|build)\//;
let options = { allowJs: false, moduleResolution: ts.ModuleResolutionKind.Bundler, module: ts.ModuleKind.ESNext, allowImportingTsExtensions: true };
const cfgPath = path.join(root, "tsconfig.json");
if (fs.existsSync(cfgPath)) { const c = ts.parseConfigFileTextToJson(cfgPath, fs.readFileSync(cfgPath, "utf8")); const co = (c.config||{}).compilerOptions||{};
  if (co.paths) options.paths = co.paths; if (co.baseUrl) options.baseUrl = path.join(root, co.baseUrl); options.pathsBasePath = root; }
const host = { fileExists: fs.existsSync.bind(fs), readFile: f => fs.readFileSync(f,"utf8"), directoryExists: d => fs.existsSync(d)&&fs.statSync(d).isDirectory(), realpath: f=>f, getCurrentDirectory: ()=>root, getDirectories: ()=>[] };
const files = graph.nodes.map(n => n.path);
const oracle = new Set(); let specifiers = 0;
for (const f of files) {
  const abs = path.join(root, f); if (!fs.existsSync(abs)) continue;
  const text = fs.readFileSync(abs, "utf8");
  const pre = ts.preProcessFile(text, true, true);
  for (const imp of pre.importedFiles) { specifiers++;
    const r = ts.resolveModuleName(imp.fileName, abs, options, host).resolvedModule;
    if (!r) continue;
    const rel = path.relative(root, fs.realpathSync(r.resolvedFileName));
    if (rel.startsWith("..") || exclude.test(rel) || rel.endsWith(".d.ts") || !/\.(ts|tsx|mts|cts)$/.test(rel)) continue;
    oracle.add(f + " -> " + rel);
  }
}
const engine = new Set(graph.edges.map(e => e.source + " -> " + e.target));
const onlyEngine = [...engine].filter(e => !oracle.has(e)); const onlyTs = [...oracle].filter(e => !engine.has(e));
console.log(`files ${files.length}  specifiers ${specifiers}  codeatlas ${engine.size}  tsc ${oracle.size}  agree ${[...engine].filter(e=>oracle.has(e)).length}`);
console.log("only codeatlas:", onlyEngine.length, onlyEngine.slice(0,6)); console.log("only tsc:", onlyTs.length, onlyTs.slice(0,8));
console.log("cov", JSON.stringify({res:graph.coverage.resolved_import_count, ext:graph.coverage.external_import_count, unres:graph.coverage.unresolved_by_reason, ts:graph.coverage.tsconfig, cycles:graph.cycles.length}));
if (onlyEngine.length || onlyTs.length) process.exit(1);
