//! Session and question state belongs to a reader, including readers of the same file.

use super::codex_facts::{Action, Fact, Template};
use super::codex_questions::QuestionOutput;
use super::{NormalizedEvent, SnapshotError, Work};
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct ReaderContext {
    session_id: String,
    questions: HashMap<String, Vec<String>>,
    outputs: HashMap<String, bool>,
}

impl ReaderContext {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn project(
        &mut self,
        fact: &Fact<'_>,
        work: &mut dyn Work,
    ) -> Result<Vec<NormalizedEvent>, SnapshotError> {
        work.check()?;
        let mut meta = fact.meta.clone();
        if let Some(id) = meta.get("session_id").and_then(|id| id.as_str()) {
            if !id.is_empty() {
                self.session_id = id.to_owned();
            }
            meta["session_id"] = self.session_id.clone().into();
        }
        let mut events = Vec::new();
        for action in &fact.actions {
            work.check()?;
            match action {
                Action::Event(event) => {
                    if event.starts_turn() {
                        self.outputs.clear();
                    }
                    events.push(event.project(&meta));
                }
                Action::Open {
                    call_id,
                    ids,
                    event,
                } => {
                    self.questions.insert(call_id.clone(), ids.clone());
                    events.push(event.project(&meta));
                }
                Action::Output {
                    call_id,
                    question,
                    event,
                    tail,
                } => {
                    let mirrored = event.is_mirror();
                    if self
                        .outputs
                        .get(call_id)
                        .is_some_and(|previous| *previous || mirrored)
                    {
                        events.push(
                            fact.ignored("item_completed/FunctionCallOutput")
                                .project(&meta),
                        );
                        continue;
                    }
                    self.outputs.insert(call_id.clone(), mirrored);
                    // Every valid string/array output consumes the call, even a custom output.
                    let ids = self.questions.remove(call_id);
                    match question {
                        QuestionOutput::Accepted if ids.is_some() => {
                            events.push(fact.ignored("question_accepted").project(&meta));
                        }
                        QuestionOutput::Answers(answers) => {
                            let mut answers = answers.clone();
                            if let Some(ids) = ids {
                                answers.sort_by_key(|answer| {
                                    ids.iter()
                                        .position(|id| *id == answer.question)
                                        .unwrap_or(ids.len())
                                });
                            }
                            events.push(
                                Template::new(
                                    "elicitation_result",
                                    json!({
                                        "tool_call_id": call_id, "answers": answers,
                                    }),
                                )
                                .project(&meta),
                            );
                        }
                        _ => {
                            events.push(event.project(&meta));
                            for event in tail {
                                work.check()?;
                                events.push(event.project(&meta));
                            }
                        }
                    }
                }
            }
        }
        Ok(events)
    }
}
