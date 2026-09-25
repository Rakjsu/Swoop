import { useEffect, useRef, useState } from 'react';

/**
 * Janela de linhas de altura fixa: só as visíveis (mais uma margem) vão para
 * o DOM, então a lista aguenta milhares de downloads sem travar.
 */
export function useVirtualRows(count: number, rowHeight: number, overscan = 6) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setHeight(el.clientHeight);
    const onScroll = () => setScrollTop(el.scrollTop);
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      observer.disconnect();
      el.removeEventListener('scroll', onScroll);
    };
  }, []);

  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const end = Math.min(count, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  return { ref, start, end, totalHeight: count * rowHeight, offset: start * rowHeight };
}
