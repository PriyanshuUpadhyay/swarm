// To-do list and plan rows: the checklist and the proposed plan that an agent CLI shows.
import type { Row, TodoItem } from "../core/api";
import type { MarkdownCache } from "../core/markdown";
import { todoText } from "../core/transcript";
import { StatusGlyph, type Status } from "./status";

const STATUS: Record<TodoItem["status"], Status> = { pending: "ended", in_progress: "working", completed: "done" };

export function TodoRow({ row }: { row: Row }) {
  return (
    <div className="todo-list" data-testid="todo-list">
      <div className="font-semibold">{row.text}</div>
      <ul>
        {row.todo?.map((item, i) => (
          <li key={i} data-status={item.status}>
            <StatusGlyph status={STATUS[item.status]} />
            <span className="todo-text">{todoText(item)}</span>
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
