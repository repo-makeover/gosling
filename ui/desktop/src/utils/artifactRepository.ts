export const ARTIFACT_REPOSITORY_BATCH_LIMIT = 200;

export interface ArtifactRepositoryClassification {
  repositoryPaths: string[];
  unavailablePaths: string[];
}

const SOURCE_EXTENSIONS = new Set([
  'bash',
  'c',
  'cc',
  'cjs',
  'clj',
  'cljs',
  'cpp',
  'cs',
  'css',
  'cxx',
  'dart',
  'ex',
  'exs',
  'fish',
  'go',
  'h',
  'hpp',
  'hs',
  'htm',
  'html',
  'hxx',
  'java',
  'jl',
  'js',
  'jsx',
  'kt',
  'kts',
  'less',
  'lua',
  'm',
  'mjs',
  'mm',
  'php',
  'pl',
  'ps1',
  'py',
  'pyi',
  'r',
  'rb',
  'rs',
  'sass',
  'scala',
  'scss',
  'sh',
  'sql',
  'svelte',
  'swift',
  'ts',
  'tsx',
  'vue',
  'zsh',
]);

const SOURCE_FILENAMES = new Set([
  '.gitignore',
  '.gitattributes',
  '.gitmodules',
  'cargo.toml',
  'cargo.lock',
  'cmakelists.txt',
  'dockerfile',
  'gemfile',
  'gemfile.lock',
  'go.mod',
  'go.sum',
  'justfile',
  'makefile',
  'package.json',
  'package-lock.json',
  'pnpm-lock.yaml',
  'pyproject.toml',
  'requirements.txt',
  'tsconfig.json',
  'yarn.lock',
]);

export function isSourceCodeFile(filePath: string): boolean {
  const name = filePath.split(/[\\/]/).pop()?.toLowerCase() ?? '';
  return (
    SOURCE_FILENAMES.has(name) ||
    (name.includes('.') && SOURCE_EXTENSIONS.has(name.split('.').pop() ?? ''))
  );
}
