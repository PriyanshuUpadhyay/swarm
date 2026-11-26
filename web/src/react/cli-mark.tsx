import { LetterBox } from "../components/ui/letter-box";

const LETTERS: Record<string, string> = { claude: "C", codex: "X", agy: "A", pi: "P" };

/** The adjacent label names the CLI. Until its product mark is cleared, the box is decorative. */
export function CliMark({ provider, selected = false }: { provider: string; selected?: boolean }) {
  if (!Object.hasOwn(LETTERS, provider)) return null;
  return (
    <span aria-hidden="true" data-cli-mark={provider} className="inline-flex shrink-0 align-middle">
      <LetterBox letter={LETTERS[provider]} className={`text-[11px] leading-none ${selected ? "text-foreground" : ""}`} />
    </span>
  );
}
