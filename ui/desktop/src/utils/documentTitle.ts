const MAX_TITLE_LENGTH = 120;
const FRONTMATTER_FENCE = /^---\s*$/;
const CODE_FENCE = /^\s*(`{3,}|~{3,})/;

function stripInlineMarkdown(value: string): string {
  return value
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/`+/g, '')
    .replace(/(\*\*|__)(.*?)\1/g, '$2')
    .replace(/(\*|_)(.*?)\1/g, '$2')
    .replace(/<[^>]+>/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function normalize(value: string): string | null {
  const title = stripInlineMarkdown(value);
  if (!title) return null;
  return title.length > MAX_TITLE_LENGTH ? `${title.slice(0, MAX_TITLE_LENGTH - 1).trimEnd()}…` : title;
}

function frontmatterTitle(lines: string[]): { title: string | null; endIndex: number } {
  if (lines.length === 0 || !FRONTMATTER_FENCE.test(lines[0])) {
    return { title: null, endIndex: 0 };
  }
  for (let index = 1; index < lines.length; index += 1) {
    if (FRONTMATTER_FENCE.test(lines[index])) {
      return { title: null, endIndex: index + 1 };
    }
    const match = /^title\s*:\s*(.+)$/i.exec(lines[index]);
    if (match) {
      const raw = match[1].trim().replace(/^["'](.*)["']$/, '$1');
      // Keep scanning to the closing fence so the body scan starts after it.
      for (let end = index + 1; end < lines.length; end += 1) {
        if (FRONTMATTER_FENCE.test(lines[end])) {
          return { title: normalize(raw), endIndex: end + 1 };
        }
      }
      return { title: normalize(raw), endIndex: lines.length };
    }
  }
  return { title: null, endIndex: 0 };
}

/**
 * The document's own title: a frontmatter `title`, else the first level-one
 * heading. Returns null when the text carries neither, so callers can fall back
 * to the file name rather than invent one.
 */
export function documentTitleFromContent(content: string): string | null {
  if (!content) return null;
  const lines = content.split(/\r?\n/);
  const frontmatter = frontmatterTitle(lines);
  if (frontmatter.title) return frontmatter.title;

  let insideCodeFence = false;
  for (let index = frontmatter.endIndex; index < lines.length; index += 1) {
    const line = lines[index];
    if (CODE_FENCE.test(line)) {
      insideCodeFence = !insideCodeFence;
      continue;
    }
    if (insideCodeFence) continue;

    const atx = /^\s{0,3}#\s+(.*\S)\s*$/.exec(line);
    if (atx) return normalize(atx[1].replace(/\s+#+\s*$/, ''));

    const isSetextUnderline = /^\s{0,3}=+\s*$/.test(line);
    if (isSetextUnderline && index > frontmatter.endIndex) {
      const previous = lines[index - 1];
      if (previous && previous.trim() && !CODE_FENCE.test(previous)) {
        return normalize(previous);
      }
    }
  }
  return null;
}

/** Titles are only meaningful for text formats the preview renders as prose. */
export function supportsDocumentTitle(filePath: string): boolean {
  const normalized = filePath.split(/[?#]/, 1)[0].toLowerCase();
  return ['.md', '.markdown', '.mdx', '.txt', '.rst'].some((extension) =>
    normalized.endsWith(extension)
  );
}
