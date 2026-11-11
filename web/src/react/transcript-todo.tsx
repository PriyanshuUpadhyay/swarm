// To-do list and plan rows: the checklist and the proposed plan that an agent CLI shows.
import type { Row, TodoItem } from "../core/api";
import { paintBodyOf, todoText } from "../core/transcript";
import { MarkdownView } from "./markdown-view";
import { StatusGlyph, type Status } from "./status";

const STATUS: Record<TodoItem["status"], Status> = { pending: "ended", in_progress: "working", completed: "done" };

export function TodoRow({ row }: { row: Row }) {
  return (
    <div className="todo-list" data-testid="todo-list">
      <div className="font-semibold">{row.text}</div>
      <ul>
        {row.todo?.map((item, i) => (
          <li key={i} data-status={item.status}>
            <StatusGlyph status={STATUS[item.status]} label={item.status === "pending" ? "Pending" : undefined} />
            <span className="todo-text">{todoText(item)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function PlanRow({ row }: { row: Row }) {
  const body = paintBodyOf(row);
  return (
    <div className="todo-list" data-testid="plan">
      <div className="font-semibold">Plan</div>
      {body.kind === "tree" ? <div className="md"><MarkdownView display={body.display} /></div> : <div className="whitespace-pre-wrap">{row.text}</div>}
    </div>
  );
}
