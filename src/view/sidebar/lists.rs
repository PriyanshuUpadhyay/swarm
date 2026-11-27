//! The two cross-project sidebar lists.

use super::*;

pub(super) fn append<'a>(
    view: &Projection<'a>,
    tree: &'a Tree,
    sections: &mut Vec<Section>,
) -> BTreeSet<&'a str> {
    let owner = view.owner;
    let state = view.state;
    let mut promoted = BTreeSet::new();

    let mut asks: Vec<_> = view
        .live_workspaces(tree)
        .flat_map(|w| {
            w.chats
                .iter()
                .filter(|c| prompt(c).is_some())
                .map(move |c| (w, c))
        })
        .collect();
    sort_chats(&mut asks);

    let count = asks.len();
    if count > 0 {
        promoted.extend(asks.iter().map(|(_, chat)| chat.id.as_str()));
        let expanded = state.expanded_lists.contains(NEEDS_YOU);
        if !expanded {
            asks.truncate(CHAT_LIMIT);
        }

        let hidden = count.saturating_sub(CHAT_LIMIT);
        let mut rows = Vec::new();
        for (w, chat) in asks {
            view.chat_rows(w, chat, None, w.chats.len() == 1, &mut rows);
        }
        if hidden > 0 {
            rows.push(more(NEEDS_YOU, if expanded { 0 } else { hidden }));
        }
        sections.push(Section {
            count: Some(count),
            ..view.group(NEEDS_YOU, "Needs you".into(), rows, true)
        });
    }

    let mut pinned = Vec::new();
    let expanded = state.expanded_lists.contains(PINNED);
    let mut seen = 0;
    let mut hidden = 0;
    for project in &tree.projects {
        for w in &project.workspaces {
            if !owner.archived.contains(&w.path) && owner.pinned.contains(&w.path) {
                let mut chats: Vec<_> = w
                    .chats
                    .iter()
                    .filter(|c| !promoted.contains(c.id.as_str()))
                    .map(|chat| (w, chat))
                    .collect();
                sort_chats(&mut chats);

                let folded = has_workspace_row(chats.len()) && state.is_collapsed(&w.path);
                let shown: Vec<_> = chats
                    .iter()
                    .copied()
                    .filter(|(_, chat)| {
                        if folded {
                            return true;
                        }
                        seen += 1;
                        let beyond_cut = seen > CHAT_LIMIT && !urgent(chat);
                        hidden += usize::from(beyond_cut);
                        expanded || !beyond_cut
                    })
                    .collect();

                view.grouped_rows(project, &shown, &chats, &mut pinned);
            }
        }
    }

    if hidden > 0 {
        pinned.push(more(PINNED, if expanded { 0 } else { hidden }));
    }
    if !pinned.is_empty() {
        sections.push(view.group(PINNED, "Pinned".into(), pinned, !state.is_collapsed(PINNED)));
    }
    promoted
}
