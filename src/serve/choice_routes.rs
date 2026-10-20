//! Server choice routes.

use super::*;
pub(super) fn apply_choice(app: &App, body: choices::ChoiceWrite) -> Result<Value, ApiError> {
    let snapshot = {
        let cache = app.tree_cache.lock().unwrap();
        if cache.snapshot.is_object() {
            cache.snapshot.clone()
        } else {
            snapshot_value(app)
                .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        }
    };
    let mut invalid = None;
    let saved = choices_store::update(&app.root, Some(&body.revision), |owner, state| {
        let tree = tree::build(&snapshot, owner);
        match choices::apply(owner, state, &tree, body.operation, discovery::now()) {
            Ok(()) => Ok(()),
            Err(e) => {
                invalid = Some(e.clone());
                Err(e)
            }
        }
    })
    .map_err(|e| {
        if let Some(message) = invalid {
            error(StatusCode::BAD_REQUEST, &message)
        } else {
            choice_error(e)
        }
    })?;
    if saved.revision != body.revision {
        app.hub.lock().unwrap().bump();
    }
    {
        let mut cache = app.tree_cache.lock().unwrap();
        cache.revision = saved.revision.clone();
        cache.saved = Some(saved.clone());
        cache.choices_error = None;
    }
    Ok(json!({"revision":saved.revision,"version":app.hub.lock().unwrap().version}))
}
pub(super) fn refresh_tree(app: &App) -> Result<Value, ApiError> {
    let saved = choices_store::load(&app.root, false).map_err(choice_error)?;
    let mut snapshot = snapshot_value(app)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    snapshot.as_object_mut().unwrap().remove("version");
    let mut discovery = app.tree_cache.lock().unwrap().discovery.clone();
    discovery.augment(&mut snapshot, &saved.owner, discovery::now());
    let first_tree = tree::build(&snapshot, &saved.owner);
    let mut discovered_owner = saved.owner.clone();
    for project in &first_tree.projects {
        if !discovered_owner.project_paths.contains(&project.path)
            && !discovered_owner
                .project_paths
                .contains(&project.launch_directory)
        {
            discovered_owner
                .project_paths
                .push(project.launch_directory.clone());
        }
    }
    let needs_projects = discovered_owner.project_paths != saved.owner.project_paths;
    let tree = tree::build(&snapshot, &discovered_owner);
    let mut normalized_state = saved.state.clone();
    normalized_state.record_first_sight(&tree);
    let needs_tabs = tree.projects.iter().flat_map(|p| &p.workspaces).any(|w| {
        saved.owner.tabs.get(&w.path).is_none_or(|strip| {
            strip.pruned(&w.chats.iter().map(|c| c.key.clone()).collect()) != *strip
        })
    });
    let needs_keys = saved.owner.clone().forget_gone_chats(&tree);
    let saved = if normalized_state != saved.state || needs_tabs || needs_projects || needs_keys {
        choices_store::update(&app.root, Some(&saved.revision), |owner, state| {
            state.record_first_sight(&tree);
            owner.forget_gone_chats(&tree);
            for p in &tree.projects {
                if !owner.project_paths.contains(&p.path)
                    && !owner.project_paths.contains(&p.launch_directory)
                {
                    owner.project_paths.push(p.launch_directory.clone());
                }
                for w in &p.workspaces {
                    let strip = owner
                        .tabs
                        .entry(w.path.clone())
                        .or_insert_with(|| w.seed_tabs());
                    *strip = strip.pruned(&w.chats.iter().map(|c| c.key.clone()).collect());
                }
            }
            Ok(())
        })
        .map_err(choice_error)?
    } else {
        saved
    };
    let fields_changed = discovery.refresh_fields(&tree, &saved.owner, discovery::now());
    let mut cache = app.tree_cache.lock().unwrap();
    cache.discovery = discovery;
    if cache.snapshot != snapshot || cache.revision != saved.revision || fields_changed {
        app.hub.lock().unwrap().bump();
    }
    cache.snapshot = snapshot;
    cache.revision = saved.revision.clone();
    cache.saved = Some(saved);
    Ok(json!({"ok":true}))
}

#[cfg(test)]
#[path = "choice_routes/tests.rs"]
mod tests;
