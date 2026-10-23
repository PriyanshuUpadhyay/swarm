use super::super::{tabs::Grouping, tree::Tree};
use super::{ChoiceOp, OwnerChoices, ViewState};

pub(super) fn apply_tab(
    owner: &mut OwnerChoices,
    state: &mut ViewState,
    tree: &Tree,
    op: ChoiceOp,
) -> Result<(), String> {
    let workspace = match &op {
        ChoiceOp::OpenTab { workspace, .. }
        | ChoiceOp::CloseTab { workspace, .. }
        | ChoiceOp::MoveTab { workspace, .. }
        | ChoiceOp::StepTab { workspace, .. }
        | ChoiceOp::TabGroupCreate { workspace, .. }
        | ChoiceOp::TabGroupRename { workspace, .. }
        | ChoiceOp::TabGroupColor { workspace, .. }
        | ChoiceOp::TabGroupDelete { workspace, .. }
        | ChoiceOp::TabGroupFold { workspace, .. }
        | ChoiceOp::MoveTabToGroup { workspace, .. } => workspace.clone(),
        _ => return Err("invalid tab operation".into()),
    };
    let (_, entry) = tree.workspace(&workspace).ok_or("unknown workspace")?;
    let strip = owner
        .tabs
        .entry(workspace.clone())
        .or_insert_with(|| entry.seed_tabs());
    let key = |id: &str| {
        entry
            .chat(id)
            .map(|c| c.key.clone())
            .ok_or_else(|| "unknown chat in workspace".to_owned())
    };
    let open = |id: &str| -> Result<String, String> {
        let k = key(id)?;
        if !strip.open.contains(&k) {
            return Err("tab is not open".into());
        }
        Ok(k)
    };
    *strip = match op {
        ChoiceOp::OpenTab { chat, .. } => {
            let current = state
                .selected_chats
                .get(&workspace)
                .and_then(|id| entry.chat(id))
                .map(|c| c.key.as_str());
            strip.opening(&key(&chat)?, current)
        }
        ChoiceOp::CloseTab { chat, .. } => strip.closing(&open(&chat)?),
        ChoiceOp::MoveTab { chat, target, .. } => {
            let key = open(&chat)?;
            let target = open(&target)?;
            strip.moving(&key, strip.open.iter().position(|k| k == &target).unwrap())
        }
        ChoiceOp::StepTab { chat, target, .. } => strip.stepping(&open(&chat)?, &open(&target)?),
        ChoiceOp::TabGroupCreate {
            id,
            name,
            color,
            chat,
            ..
        } => {
            if !super::super::tabs::valid_color(&color) {
                return Err("unknown group color".into());
            }
            strip.grouping(Grouping::New {
                id,
                name,
                color,
                tab: open(&chat)?,
            })
        }
        ChoiceOp::TabGroupRename { group, name, .. } => {
            strip.grouping(Grouping::Rename { group, name })
        }
        ChoiceOp::TabGroupColor { group, color, .. } => {
            if !super::super::tabs::valid_color(&color) {
                return Err("unknown group color".into());
            }
            strip.grouping(Grouping::Color { group, color })
        }
        ChoiceOp::TabGroupDelete { group, .. } => strip.grouping(Grouping::Delete(group)),
        ChoiceOp::TabGroupFold { group, folded, .. } => {
            strip.grouping(Grouping::Fold { group, folded })
        }
        ChoiceOp::MoveTabToGroup { chat, group, .. } => {
            let tab = open(&chat)?;
            strip.grouping(group.map_or_else(
                || Grouping::Remove(tab.clone()),
                |group| Grouping::Add {
                    tab: tab.clone(),
                    group,
                },
            ))
        }
        _ => unreachable!(),
    };
    if let Some(history) = state.tab_history.get_mut(&workspace) {
        history.retain(|k| strip.open.contains(k));
    }
    let selected = state
        .selected_chats
        .get(&workspace)
        .and_then(|id| entry.chat(id));
    if selected.is_none_or(|c| !strip.open.contains(&c.key)) {
        let key = strip.selection_after_close(
            state
                .tab_history
                .get(&workspace)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        if let Some(chat) = key.and_then(|k| entry.chat(k)) {
            state.selected_chats.insert(workspace, chat.id.clone());
        } else {
            state.selected_chats.remove(&workspace);
        }
    }
    Ok(())
}
