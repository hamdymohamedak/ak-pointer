import { mkdir } from "node:fs/promises";

await mkdir("dist", { recursive: true });
await Bun.write("dist/index.html", await Bun.file("src/renderer/index.html").text());

const builds = [
  { entry: "src/main.ts", target: "node" as const, format: "cjs" as const },
  { entry: "src/preload.ts", target: "node" as const, format: "cjs" as const },
  { entry: "src/renderer/renderer.ts", target: "browser" as const, format: "iife" as const },
];

for (const build of builds) {
  const result = await Bun.build({
    entrypoints: [build.entry],
    outdir: "dist",
    target: build.target,
    format: build.format,
    external: ["electron"],
  });
  if (!result.success) {
    for (const log of result.logs) console.error(log);
    process.exit(1);
  }
}
