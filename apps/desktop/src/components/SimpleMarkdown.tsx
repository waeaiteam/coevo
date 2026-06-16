/**
 * SimpleMarkdown — lightweight markdown-to-JSX renderer.
 * Handles: headings, bold, italic, inline code, code blocks, lists, hr, paragraphs.
 * No external dependencies — just regex + React elements.
 */

import type { ReactNode } from "react";

interface Props {
  content: string;
  className?: string;
}

export function SimpleMarkdown({ content, className = "" }: Props) {
  const blocks = parseBlocks(content);
  return (
    <div className={`markdown-render ${className}`}>
      {blocks.map((block, i) => (
        <Block key={i} block={block} />
      ))}
    </div>
  );
}

type BlockNode =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "code"; lang: string; text: string }
  | { type: "hr" };

function parseBlocks(src: string): BlockNode[] {
  const lines = src.split("\n");
  const blocks: BlockNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Blank line
    if (line.trim() === "") { i++; continue; }

    // HR
    if (/^---+$/.test(line.trim())) { blocks.push({ type: "hr" }); i++; continue; }

    // Heading
    const headingMatch = line.match(/^(#{1,3})\s+(.*)$/);
    if (headingMatch) {
      blocks.push({ type: "heading", level: headingMatch[1].length, text: headingMatch[2] });
      i++; continue;
    }

    // Fenced code block
    if (line.trim().startsWith("```")) {
      const lang = line.trim().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      i++; // skip closing ```
      blocks.push({ type: "code", lang, text: codeLines.join("\n") });
      continue;
    }

    // Unordered list
    if (/^[-*]\s/.test(line.trim())) {
      const items: string[] = [];
      while (i < lines.length && /^[-*]\s/.test(lines[i].trim())) {
        items.push(lines[i].trim().replace(/^[-*]\s+/, ""));
        i++;
      }
      blocks.push({ type: "list", ordered: false, items });
      continue;
    }

    // Ordered list
    if (/^\d+\.\s/.test(line.trim())) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\.\s/.test(lines[i].trim())) {
        items.push(lines[i].trim().replace(/^\d+\.\s+/, ""));
        i++;
      }
      blocks.push({ type: "list", ordered: true, items });
      continue;
    }

    // Paragraph (collect consecutive non-empty, non-special lines)
    const paraLines: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !/^#{1,3}\s/.test(lines[i]) && !/^[-*]\s/.test(lines[i].trim()) && !/^\d+\.\s/.test(lines[i].trim()) && !/^---+$/.test(lines[i].trim()) && !lines[i].trim().startsWith("```")) {
      paraLines.push(lines[i]);
      i++;
    }
    if (paraLines.length > 0) {
      blocks.push({ type: "paragraph", text: paraLines.join("\n") });
    }
  }
  return blocks;
}

function Block({ block }: { block: BlockNode }) {
  switch (block.type) {
    case "hr": return <hr />;
    case "heading": {
      const Tag = `h${block.level}` as "h1" | "h2" | "h3";
      return <Tag>{inlineFormat(block.text)}</Tag>;
    }
    case "paragraph": return <p>{inlineFormat(block.text)}</p>;
    case "code": return <pre><code>{block.text}</code></pre>;
    case "list": {
      const Tag = block.ordered ? "ol" : "ul";
      return <Tag>{block.items.map((item, idx) => <li key={idx}>{inlineFormat(item)}</li>)}</Tag>;
    }
  }
}

function inlineFormat(text: string): ReactNode[] {
  // Split on inline patterns: **bold**, *italic*, `code`
  const parts: ReactNode[] = [];
  const regex = /(\*\*(.+?)\*\*)|(\*(.+?)\*)|(`(.+?)`)/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    if (match[1]) {
      parts.push(<strong key={match.index}>{match[2]}</strong>);
    } else if (match[3]) {
      parts.push(<em key={match.index}>{match[4]}</em>);
    } else if (match[5]) {
      parts.push(<code key={match.index}>{match[6]}</code>);
    }
    lastIndex = match.index + match[0].length;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts.length > 0 ? parts : [text];
}
