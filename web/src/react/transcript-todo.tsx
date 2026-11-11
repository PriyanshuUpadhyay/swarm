// To-do list and plan rows: the checklist and the proposed plan that an agent CLI shows.
import { CheckIcon, CircleIcon, LoaderCircleIcon } from "lucide-react";
import type { Row, TodoItem } from "../core/api";
import type { MarkdownCache } from "../core/markdown";

function Glyph({ status }: { status: TodoItem["status"] }) {
  if (status === "completed") return <CheckIcon aria-label="Done" className="glyph-slot text-done" />;
  if (status === "in_progress") return <LoaderCircleIcon aria-label="In progress" className="glyph-slot working" />;
  return <CircleIcon aria-label="Pending" className="glyph-slot text-muted-foreground" />;
}

export function TodoRow({ row }: { row: Row }) {
  return (
    <div className="todo-list" data-testid="todo-list">
      <div className="font-semibold">{row.text}</div>
      <ul>
        {row.todo?.map((item, i) => (
          <li key={i} data-status={item.status}>
            <Glyph status={item.status} />
            <span className="todo-text">{item.status === "in_progress" && item.active_text ? item.active_text : item.text}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function PlanRow({ row, md }: { row: Row; md: MarkdownCache }) {
  return (
    <div className="todo-list" data-testid="plan">
      <div className="font-semibold">Plan</div>
      <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text) }} />
    </div>
  );
}
