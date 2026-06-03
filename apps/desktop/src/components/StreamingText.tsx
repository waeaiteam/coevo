import { useEffect, useRef, useState } from 'react';

interface StreamingTextProps {
  content: string;
  isStreaming: boolean;
  className?: string;
  typingSpeed?: number;
}

export function StreamingText({
  content,
  isStreaming,
  className = '',
  typingSpeed = 0,
}: StreamingTextProps) {
  const [displayedContent, setDisplayedContent] = useState('');
  const contentRef = useRef(content);
  const indexRef = useRef(0);

  useEffect(() => {
    contentRef.current = content;

    if (!isStreaming || typingSpeed === 0) {
      setDisplayedContent(content);
      indexRef.current = content.length;
      return;
    }

    const interval = setInterval(() => {
      if (indexRef.current < contentRef.current.length) {
        indexRef.current++;
        setDisplayedContent(contentRef.current.slice(0, indexRef.current));
      } else {
        clearInterval(interval);
      }
    }, typingSpeed);

    return () => clearInterval(interval);
  }, [content, isStreaming, typingSpeed]);

  return (
    <div className={className}>
      {displayedContent}
      {isStreaming && <span className="streaming-cursor">▊</span>}
    </div>
  );
}
