import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const src = path.join(root, "src");
const dist = path.join(root, "dist");

const failures: string[] = [];

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = path.join(dir, entry);
    return statSync(full).isDirectory() ? walk(full) : [full];
  });
}

function read(relative: string): string {
  return readFileSync(path.join(root, relative), "utf8");
}

const sourceFiles = walk(src).filter((file) => /\.(ts|tsx|css)$/.test(file));
const joinedSource = sourceFiles.map((file) => readFileSync(file, "utf8")).join("\n");

for (const forbidden of ["dangerouslySetInnerHTML", ".innerHTML", "insertAdjacentHTML", "new Function", "eval("]) {
  if (joinedSource.includes(forbidden)) {
    failures.push(`forbidden unsafe sink found: ${forbidden}`);
  }
}

const sectionUses = joinedSource.match(/<Section\b/g)?.length ?? 0;
const sectionQuestionUses = joinedSource.match(/<Section\b[^>]*\bquestions=\{/g)?.length ?? 0;
if (sectionUses === 0) {
  failures.push("no Section primitives found");
}
if (sectionUses !== sectionQuestionUses) {
  failures.push(`every Section must declare questions (${sectionQuestionUses}/${sectionUses})`);
}

const rawValueUses = joinedSource.match(/<RawValue\b/g)?.length ?? 0;
const defaultOpenRaw = joinedSource.match(/<RawValue\b[^>]*\bdefaultOpen/g)?.length ?? 0;
if (defaultOpenRaw > 0) {
  failures.push("RawValue disclosure must not default open");
}
if (rawValueUses === 0) {
  failures.push("no collapsed raw verification surface found");
}

const checklist = read("VIEW_AUTHOR_CHECKLIST.md");
for (const required of ["tier", "questions", "collapsed disclosure", "freshness", "keyboard"]) {
  if (!checklist.toLowerCase().includes(required)) {
    failures.push(`checklist missing ${required}`);
  }
}

try {
  const distFiles = walk(dist);
  for (const file of distFiles) {
    const text = readFileSync(file, "utf8");
    const relative = path.relative(root, file);
    if (/\/\/fonts\.|googleapis|gstatic|cdn\./i.test(text)) {
      failures.push(`external asset host found in ${relative}`);
    }
    if (relative.endsWith(".html") && /\b(?:src|href)=["']https?:\/\//i.test(text)) {
      failures.push(`external HTML resource found in ${relative}`);
    }
    if (relative.endsWith(".css") && /(?:@import\s+|url\()["']?https?:\/\//i.test(text)) {
      failures.push(`external CSS resource found in ${relative}`);
    }
    if (
      relative.endsWith(".js") &&
      /(?:fetch|XMLHttpRequest|WebSocket|EventSource)\s*\([^)]*["']https?:\/\//i.test(text)
    ) {
      failures.push(`external JS network request found in ${relative}`);
    }
  }
} catch {
  failures.push("dist is missing; run bun run build before charter check");
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(`dashboard charter check ok: sections=${sectionUses} raw_disclosures=${rawValueUses}`);
