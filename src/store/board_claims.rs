//! Board claims (ADR 0115): one holder per work key, a generation fence, and an attempt cap.

use super::*;
use std::ops::ControlFlow;

/// After this many holders, a work key parks for the owner instead of taking another claim.
const MAX_ATTEMPTS: i64 = 3;
const MAX_LEASE_SECS: i64 = 86_400;

/// Takes, renews, or releases the claim that a claim or handoff post names. `Break` carries a
/// renewal, which writes no post; `Continue` carries the post's generation.
pub(super) fn claim_fence(
    tx: &Connection,
    project_id: &str,
    session_id: &str,
    author: &str,
    now: i64,
    new: &NewPost,
) -> Result<ControlFlow<Posted, Option<i64>>, Box<dyn std::error::Error>> {
    let mut generation = None;
    match new.kind {
        "claim" => {
            let work = new.work_key.ok_or("swarm: a claim needs --work <key>")?;
            let lease = new
                .lease_secs
                .ok_or("swarm: a claim needs --lease <seconds>")?;
            if !(1..=MAX_LEASE_SECS).contains(&lease) {
                return Err(format!("swarm: --lease is 1 to {MAX_LEASE_SECS} seconds").into());
            }
            let current = claim_row(tx, project_id, work)?;
            match current {
                None => {
                    tx.execute(
                        "INSERT INTO claim (project_id, work_key, session_id, holder, generation, attempts, lease_until)
                         VALUES (?1, ?2, ?3, ?4, 1, 1, ?5)",
                        (project_id, work, session_id, author, now + lease),
                    )?;
                    generation = Some(1);
                }
                Some(c) => {
                    let holds =
                        c.session_id == session_id && c.holder == author && c.released_at.is_none();
                    if holds {
                        return match new.generation {
                            Some(g) if g == c.generation => {
                                tx.execute(
                                    "UPDATE claim SET lease_until = ?3 WHERE project_id = ?1 AND work_key = ?2",
                                    (project_id, work, now + lease),
                                )?;
                                Ok(ControlFlow::Break(Posted::Renewed {
                                    generation: g,
                                    lease_until: now + lease,
                                }))
                            }
                            Some(g) => Err(format!(
                                "swarm: stale generation {g}; current is {}",
                                c.generation
                            )
                            .into()),
                            None => Err(format!(
                                "swarm: you hold {work}, generation {}; renew with --generation {}",
                                c.generation, c.generation
                            )
                            .into()),
                        };
                    }
                    if c.released_at.is_none() && c.lease_until > now {
                        return Err(format!(
                            "swarm: held by {} for {} s more, generation {}",
                            c.holder,
                            c.lease_until - now,
                            c.generation
                        )
                        .into());
                    }
                    if c.attempts >= MAX_ATTEMPTS {
                        return Err(format!(
                            "swarm: {MAX_ATTEMPTS} attempts used on {work}; parked for owner"
                        )
                        .into());
                    }
                    tx.execute(
                        "UPDATE claim SET session_id = ?3, holder = ?4, generation = generation + 1,
                                attempts = attempts + 1, lease_until = ?5, released_at = NULL
                         WHERE project_id = ?1 AND work_key = ?2",
                        (project_id, work, session_id, author, now + lease),
                    )?;
                    generation = Some(c.generation + 1);
                }
            }
        }
        "handoff" => {
            let work = new.work_key.ok_or("swarm: a handoff needs --work <key>")?;
            let g = new
                .generation
                .ok_or("swarm: a handoff needs --generation <n> from your claim")?;
            let c = claim_row(tx, project_id, work)?
                .ok_or_else(|| format!("swarm: nobody claimed {work}"))?;
            if c.generation != g {
                return Err(
                    format!("swarm: stale generation {g}; current is {}", c.generation).into(),
                );
            }
            if c.session_id != session_id || c.holder != author || c.released_at.is_some() {
                return Err(format!("swarm: you do not hold {work}").into());
            }
            // The checker records its test before a worker's work counts as handed off (ADR 0115).
            let role: String = tx.query_row(
                "SELECT role FROM agent WHERE session_id = ?1 AND id = ?2",
                (session_id, author),
                |r| r.get(0),
            )?;
            let tested: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM post WHERE project_id = ?1 AND work_key = ?2
                                AND kind = 'finding' AND author_role = ?3)",
                (project_id, work, CHECKER_ROLE),
                |r| r.get(0),
            )?;
            if !tested && role != CHECKER_ROLE && !is_lead(tx, session_id, author)? {
                return Err(format!(
                    "swarm: no checker test recorded for {work}; the checker posts a finding with --work {work} first"
                )
                .into());
            }
            tx.execute(
                "UPDATE claim SET released_at = ?3 WHERE project_id = ?1 AND work_key = ?2",
                (project_id, work, now),
            )?;
            generation = Some(g);
        }
        _ => {}
    }
    Ok(ControlFlow::Continue(generation))
}

pub(super) fn claim_row(
    tx: &Connection,
    project_id: &str,
    work: &str,
) -> rusqlite::Result<Option<Claim>> {
    tx.query_row(
        "SELECT work_key, session_id, holder, generation, attempts, lease_until, released_at
         FROM claim WHERE project_id = ?1 AND work_key = ?2",
        (project_id, work),
        claim_from,
    )
    .optional()
}

pub(super) fn claim_from(r: &rusqlite::Row) -> rusqlite::Result<Claim> {
    Ok(Claim {
        work_key: r.get(0)?,
        session_id: r.get(1)?,
        holder: r.get(2)?,
        generation: r.get(3)?,
        attempts: r.get(4)?,
        lease_until: r.get(5)?,
        released_at: r.get(6)?,
    })
}

pub fn claims(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<Claim>, Box<dyn std::error::Error>> {
    let mut statement = connection.prepare(
        "SELECT work_key, session_id, holder, generation, attempts, lease_until, released_at
         FROM claim WHERE project_id = ?1 ORDER BY work_key",
    )?;
    let rows = statement.query_map([project_id], claim_from)?;
    Ok(rows.collect::<Result<_, _>>()?)
}
