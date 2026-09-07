import { describe, expect, it } from 'vitest';

import { documentTitleFromContent, supportsDocumentTitle } from './documentTitle';

describe('documentTitleFromContent', () => {
  it('returns the first level-one heading', () => {
    expect(documentTitleFromContent('# Numerical source-consistency sweep\n\nBody')).toBe(
      'Numerical source-consistency sweep'
    );
  });

  it('prefers a frontmatter title over a later heading', () => {
    const content = ['---', 'title: Seam audit', 'author: eric', '---', '', '# Ignored'].join('\n');
    expect(documentTitleFromContent(content)).toBe('Seam audit');
  });

  it('unquotes a frontmatter title', () => {
    expect(documentTitleFromContent('---\ntitle: "Works ledger"\n---\n')).toBe('Works ledger');
  });

  it('skips frontmatter without a title and reads the heading after it', () => {
    const content = ['---', 'author: eric', '---', '', '# Synthesis'].join('\n');
    expect(documentTitleFromContent(content)).toBe('Synthesis');
  });

  it('ignores headings inside fenced code blocks', () => {
    const content = ['```sh', '# not a title', '```', '', '# Real title'].join('\n');
    expect(documentTitleFromContent(content)).toBe('Real title');
  });

  it('reads a setext underlined heading', () => {
    expect(documentTitleFromContent('Review protocol\n===============\n\nBody')).toBe(
      'Review protocol'
    );
  });

  it('strips inline markdown and collapses whitespace', () => {
    expect(documentTitleFromContent('# **Bounded**   `edition` [report](./r.md)')).toBe(
      'Bounded edition report'
    );
  });

  it('does not treat a deeper heading as the title', () => {
    expect(documentTitleFromContent('## Subsection\n\nBody')).toBeNull();
  });

  it('returns null for content with no title', () => {
    expect(documentTitleFromContent('Just a paragraph.\n')).toBeNull();
    expect(documentTitleFromContent('')).toBeNull();
  });

  it('truncates an overlong title', () => {
    const title = documentTitleFromContent(`# ${'a'.repeat(200)}`);
    expect(title).toHaveLength(120);
    expect(title?.endsWith('…')).toBe(true);
  });
});

describe('supportsDocumentTitle', () => {
  it('accepts prose formats', () => {
    expect(supportsDocumentTitle('/tmp/REPORT.md')).toBe(true);
    expect(supportsDocumentTitle('notes.MARKDOWN')).toBe(true);
    expect(supportsDocumentTitle('log.txt')).toBe(true);
  });

  it('rejects formats whose first heading is not a title', () => {
    expect(supportsDocumentTitle('data.csv')).toBe(false);
    expect(supportsDocumentTitle('chart.png')).toBe(false);
    expect(supportsDocumentTitle('report.pdf')).toBe(false);
  });
});
