import { useEffect, useEffectEvent, useRef, useState, type RefObject } from "react";

/** Find belongs to the diff or terminal that has the keyboard. */
export function SurfaceFind({ surface, scope, search }: { surface: RefObject<HTMLElement | null>; scope: string; search: (query: string, direction: number) => string }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [count, setCount] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const show = useEffectEvent(() => { setOpen(true); setCount(search(query, 0)); requestAnimationFrame(() => input.current?.select()); });
  useEffect(() => {
    const el = surface.current;
    el?.addEventListener("swarm:find", show);
    return () => el?.removeEventListener("swarm:find", show);
  }, [surface]);
  if (!open) return null;
  return <div className="find" role="search" aria-label={`Find in ${scope}`}>
    <input ref={input} autoFocus aria-label={`Find in ${scope}`} placeholder={`Find in ${scope}`} value={query} onChange={(e) => { setQuery(e.target.value); setCount(search(e.target.value, 0)); }} onKeyDown={(e) => {
      if (e.key === "Enter") { e.preventDefault(); setCount(search(query, e.shiftKey ? -1 : 1)); }
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); search("", 0); setOpen(false); surface.current?.focus(); }
    }} />
    <span className="text-xs">{count}</span>
    <button type="button" aria-label={`Close ${scope} find`} onClick={() => { search("", 0); setOpen(false); surface.current?.focus(); }}>Close</button>
  </div>;
}
