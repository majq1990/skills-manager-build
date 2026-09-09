#!/usr/bin/env node

import fs from "node:fs";

const [inputPath, outputPath = inputPath, artifactDir, baseUrlArg] = process.argv.slice(2);

if (!inputPath) {
  console.error(
    "Usage: node scripts/normalize-updater-manifest.mjs <latest.json> [output.json] [artifact-dir] [base-url]",
  );
  process.exit(1);
}

const manifest = JSON.parse(fs.readFileSync(inputPath, "utf8"));
const platforms = manifest.platforms;
const nsis = platforms?.["windows-x86_64-nsis"];
const sigDir = artifactDir ?? inputPath.replace(/[\\/]latest\.json$/i, "");

if (!nsis?.url || !nsis?.signature) {
  throw new Error(
    'latest.json is missing a signed "windows-x86_64-nsis" updater entry',
  );
}

// Older clients (including 1.24.1) request the universal Windows target.
// Make that target resolve to NSIS instead of MSI, which cannot complete the
// silent updater flow used by those clients. Keep the explicit MSI entry for
// manual installation and newer clients that request it directly.
const fileNameFromUrl = (url, fallback) => {
  try {
    return decodeURIComponent(new URL(url).pathname.split("/").pop()) || fallback;
  } catch {
    return fallback;
  }
};
const nsisFile = fileNameFromUrl(
  nsis.url,
  `skills-manager_${manifest.version}_x64-setup.exe`,
);
const msiFile = fileNameFromUrl(
  platforms["windows-x86_64-msi"]?.url,
  `skills-manager_${manifest.version}_x64_en-US.msi`,
);
const readSignature = (filename, fallback) => {
  const path = `${sigDir}/${filename}`;
  return fs.existsSync(path) ? fs.readFileSync(path, "utf8").trim() : fallback;
};
const nsisSignature = readSignature(`${nsisFile}.sig`, nsis.signature);
const msiSignature = readSignature(
  `${msiFile}.sig`,
  platforms["windows-x86_64-msi"]?.signature,
);
const resolveUrl = (filename, existingUrl) =>
  baseUrlArg ? `${baseUrlArg.replace(/\/$/, "")}/${filename}` : existingUrl;

const fixedNsis = {
  signature: nsisSignature || nsis.signature,
  url: resolveUrl(nsisFile, nsis.url),
};
const fixedMsi = {
  signature: msiSignature || platforms["windows-x86_64-msi"]?.signature,
  url: resolveUrl(msiFile, platforms["windows-x86_64-msi"]?.url),
};

platforms["windows-x86_64"] = fixedNsis;
platforms["windows-x86_64-nsis"] = fixedNsis;
platforms["windows-x86_64-msi"] = fixedMsi;

fs.writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
