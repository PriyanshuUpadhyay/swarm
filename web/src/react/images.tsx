// Previews of pasted images, in the composer and in a sent message. A click shows one full size.
import { XIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { imagePaths } from "../core/images";

type Load = (path: string) => Promise<string>;

function Thumb({ path, load, onRemove }: { path: string; load: Load; onRemove?: () => void }) {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    let live = true;
    load(path).then(
      (u) => live && setUrl(u),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, [load, path]);
  const name = path.slice(path.lastIndexOf("/") + 1);
  return (
    <div className="group relative" data-testid="image-thumb">
      <button
        type="button"
        className="block size-16 overflow-hidden rounded-md border border-border bg-muted"
        aria-label="Show full size"
        title={failed ? "The image could not load" : "Show full size"}
        disabled={!url}
        onClick={() => setOpen(true)}
      >
        {url ? <img src={url} alt={name} className="size-full object-cover" /> : <span className="text-xs text-faint-foreground">{failed ? "!" : "…"}</span>}
      </button>
      {onRemove && (
        <button
          type="button"
          aria-label="Remove image"
          className="absolute -top-1.5 -right-1.5 hidden size-5 items-center justify-center rounded-full border border-border bg-popover group-hover:flex"
          onClick={onRemove}
        >
          <XIcon className="size-3" />
        </button>
      )}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="top-1/2 w-auto max-w-[90vw] -translate-y-1/2 p-2" data-testid="image-full" aria-describedby={undefined}>
          <DialogTitle className="sr-only">{name}</DialogTitle>
          {url && <img src={url} alt={name} className="max-h-[85vh] max-w-[calc(90vw-16px)] object-contain" />}
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** The previews for the images that `text` names. `remove` drops one path from a draft. */
export function ImageThumbs({ text, load, remove }: { text: string; load: Load; remove?: (path: string) => void }) {
  const paths = imagePaths(text);
  if (!paths.length) return null;
  return (
    <div className="flex flex-wrap gap-2 px-3 pt-2">
      {paths.map((p) => (
        <Thumb key={p} path={p} load={load} onRemove={remove && (() => remove(p))} />
      ))}
    </div>
  );
}
