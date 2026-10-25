//! Command dispatch and CLI helpers.

use std::env;
use swarm::providers::Provider;

#[cfg(feature = "serve")]
use crate::serve;

mod accounts;
pub(crate) use accounts::*;

mod answer;
pub(crate) use answer::*;

mod chair_idle;
pub(crate) use chair_idle::*;

mod close;
pub(crate) use close::*;

mod commands_info;
pub(crate) use commands_info::*;

mod commands_launch;
pub(crate) use commands_launch::*;

mod commands_pane;
pub(crate) use commands_pane::*;

mod commands_session;
pub(crate) use commands_session::*;

mod conductor_import;
pub(crate) use conductor_import::*;

mod context;
pub(crate) use context::*;

mod delivery;
pub(crate) use delivery::*;

mod dispatch;
pub(crate) use dispatch::*;

mod doctor;
pub(crate) use doctor::*;

mod hooks;
pub(crate) use hooks::*;

mod listing;
pub(crate) use listing::*;

mod logs;
pub(crate) use logs::*;

mod maintenance;
pub(crate) use maintenance::*;

mod managed_hooks;
pub(crate) use managed_hooks::*;

mod notices;
pub(crate) use notices::*;

mod permission;
pub(crate) use permission::*;

mod read_screen;
pub(crate) use read_screen::*;

mod ring;
pub(crate) use ring::*;

mod roles;
pub(crate) use roles::*;

mod setup;
pub(crate) use setup::*;

mod setup_plan;
pub(crate) use setup_plan::*;

mod spawn;
pub(crate) use spawn::*;

mod sweep;
pub(crate) use sweep::*;

mod trust;
pub(crate) use trust::*;

#[cfg(test)]
mod test_support;
