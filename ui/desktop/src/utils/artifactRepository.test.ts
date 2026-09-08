import { describe, expect, it } from 'vitest';
import { isSourceCodeFile } from './artifactRepository';

describe('repository output filenames', () => {
  it.each([
    '/outputs/main.rs',
    'C:\\work\\Component.TSX',
    '/tmp/script.swift',
    '/tmp/package.json',
    '/tmp/requirements.txt',
    '/tmp/pnpm-lock.yaml',
    '/tmp/Makefile',
  ])('recognizes source code and project files: %s', (filePath) => {
    expect(isSourceCodeFile(filePath)).toBe(true);
  });

  it.each([
    '/outputs/report.md',
    '/outputs/data.json',
    '/outputs/results.yaml',
    '/outputs/diagram.svg',
    '/outputs/report.pdf',
    '/outputs/main.rs.txt',
    '/outputs/ts',
    '/home/src/documents/notes.txt',
  ])('does not infer repository membership for ordinary deliverables: %s', (filePath) => {
    expect(isSourceCodeFile(filePath)).toBe(false);
  });
});
