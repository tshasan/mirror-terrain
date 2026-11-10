#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)]

mod probes;
mod scorecard;

use mt_bots::{attacks, draft_round, make_bot, shape_builds, Bot, BOT_NAMES};
use mt_core::rand::{Rng as _, SeedableRng};
use mt_core::*;
use probes::{Instr, Probe};
use std::time::Instant;

/// The rules version every output is labelled with. Bump it with any rules change that moves the golden hashes.
pub const VERSION: &str = "0.5";

const OPS: [&str; 7] = ["raise", "lower", "wall", "flood", "grow", "clear", "expose"];

#[derive(Clone, Debug)]
struct GameResult {
    names: [String; 2],
    winner: usize,
    rounds: u32,
    first_dmg: Option<u32>,
    hp_r2: Option<[i32; 2]>,
    tp_by_op: [[i32; 7]; 2],
    count_by_op: [[i32; 7]; 2],
    reverted: [i32; 2],
    tp_spent: [i32; 2],
    tp_avail: [i32; 2],
    hp_traj: Vec<[i32; 2]>,
    hash: u64,
    commanders: [usize; 2],
    scars: i32,
    scar_rounds: i32,
    draft_collisions: i32,
    /// Shaping decisions by a searching shaper that had a legal op, and those where it chose none.
    nohelp_eligible: [i32; 2],
    nohelp_none: [i32; 2],
    legal_ops: [i32; 2],
    built: [i32; 2],
    bought: [i32; 2],
    /// Rounds from round 2 whose crossing equals the previous round's, and the rounds counted.
    cross_repeat: [i32; 2],
    cross_rounds: [i32; 2],
    probe: Probe,
}

/// Bots whose shaping is a search over every legal op, so "it did nothing" means "no op helped".
fn shapes_by_search(name: &str) -> bool {
    name.starts_with("shaper") || name.starts_with("shape_only")
}

/// One game. The phase order is the one in `mt_bots::play_round_after_draft`, opened up for the instrumentation.
fn play(seed: u64, a: &mut dyn Bot, b: &mut dyn Bot, record: bool, instr: Instr) -> (GameResult, Option<String>) {
    let mut g = Game::new(seed);
    if record {
        g = g.with_recording();
    }
    let searcher = [a.name(), b.name()].map(|n| shapes_by_search(&n));
    let (mut legal_ops, mut built, mut bought) = ([0; 2], [0; 2], [0; 2]);
    let (mut nohelp_eligible, mut nohelp_none) = ([0; 2], [0; 2]);
    let (mut cross_repeat, mut cross_rounds) = ([0; 2], [0; 2]);
    let mut prev_cross: Option<[u8; 2]> = None;
    let mut probe = Probe::default();
    while !g.is_over() {
        g.start_round();
        draft_round(&mut g, a, b);
        let plans = [a.terraform(&g, 0), b.terraform(&g, 1)];
        let builds = shape_builds(&g, &plans, a, b);
        let mut clean: [Vec<(usize, Op)>; 2] = [vec![], vec![]];
        if instr != Instr::Off {
            for side in 0..2 {
                clean[side] = g.sanitize_plan(side, &plans[side]);
                legal_ops[side] += g.legal_ops(side).len() as i32;
                built[side] += builds[side].buildings.len() as i32;
                bought[side] += builds[side].units.len() as i32;
                if let Some(none) = probes::no_op_helps(&g, side, &plans[side]).filter(|_| searcher[side]) {
                    nohelp_eligible[side] += 1;
                    nohelp_none[side] += i32::from(none);
                }
            }
        }
        let pre = (instr == Instr::Full).then(|| g.sim_clone());
        g.apply_terraform(plans);
        g.apply_build(builds.clone());
        let cross = attacks(&g, a, b);
        if let Some(prev) = prev_cross {
            for side in 0..2 {
                cross_rounds[side] += 1;
                cross_repeat[side] += i32::from(prev[side] == cross[side]);
            }
        }
        prev_cross = Some(cross);
        let built_game = (instr == Instr::Full).then(|| g.sim_clone());
        g.apply_attack(cross);
        g.clash_and_resolve();
        if let (Some(pre), Some(built_game)) = (&pre, &built_game) {
            probes::probe_round(pre, &clean, &builds, built_game, cross, &g, &mut probe);
        }
    }
    let s = &g.stats;
    let r = GameResult {
        names: [a.name(), b.name()],
        winner: g.winner(),
        rounds: g.round,
        first_dmg: s.first_core_damage_round,
        hp_r2: s.core_hp_after_round.get(1).copied(),
        tp_by_op: [s.side[0].tp_by_op, s.side[1].tp_by_op],
        count_by_op: [s.side[0].count_by_op, s.side[1].count_by_op],
        reverted: [s.side[0].reverted, s.side[1].reverted],
        tp_spent: [s.side[0].tp_spent, s.side[1].tp_spent],
        tp_avail: [s.side[0].tp_available, s.side[1].tp_available],
        hp_traj: s.core_hp_after_round.clone(),
        hash: g.hash(),
        commanders: [g.players[0].commander as usize, g.players[1].commander as usize],
        scars: s.craters + s.ruins,
        scar_rounds: s.scar_rounds,
        draft_collisions: s.draft_collisions,
        nohelp_eligible,
        nohelp_none,
        legal_ops,
        built,
        bought,
        cross_repeat,
        cross_rounds,
        probe,
    };
    let json = g.rec.as_ref().map(|rec| {
        let (n0, n1) = (a.name(), b.name());
        rec.finish(g.winner(), [&n0, &n1], [g.players[0].commander.name(), g.players[1].commander.name()])
    });
    (r, json)
}

fn thread_count() -> usize {
    std::env::var("MT_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2))
}

/// Run `f(0..n)` on `threads` workers that pull the next index from a shared counter, so fast cores take more of the
/// work than slow ones. Slot `i` of the result holds `f(i)` whatever the timing (`None` only if a worker panicked), so
/// the output never depends on scheduling.
fn par_indexed<T: Send>(n: usize, threads: usize, f: impl Fn(usize) -> T + Sync) -> Vec<Option<T>> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut slots: Vec<Option<T>> = (0..n).map(|_| None).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads.min(n))
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        out.push((i, f(i)));
                    }
                    out
                })
            })
            .collect();
        for h in handles {
            for (i, v) in h.join().unwrap_or_default() {
                slots[i] = Some(v);
            }
        }
    });
    slots
}

/// Seat-swapped head-to-head: each seed is played twice, and the two bots (each with its own bot seed) trade seats
/// for the second game. That holds for a mirror too, so self-play gives two different games per deal. Results come back
/// in seed order, so sums over them do not depend on the thread count.
fn head_to_head(x: &str, y: &str, games: usize, base_seed: u64, threads: usize, instr: Instr) -> Vec<GameResult> {
    par_indexed(games / 2, threads, |i| {
        let seed = base_seed + i as u64;
        let bots = [(x, seed.wrapping_mul(7919) + 1), (y, seed.wrapping_mul(7919) + 2)];
        [false, true].map(|swap| {
            let (sa, sb) = if swap { (bots[1], bots[0]) } else { (bots[0], bots[1]) };
            let mut a = make_bot(sa.0, sa.1);
            let mut b = make_bot(sb.0, sb.1);
            play(seed, a.as_mut(), b.as_mut(), false, instr).0
        })
    })
    .into_iter()
    .flatten()
    .flatten()
    .collect()
}

#[derive(Clone, Copy)]
struct Rate {
    p: f64,
    ci: f64,
    n: usize,
}

fn rate_ci(w: f64, n: usize) -> (f64, f64) {
    if n == 0 {
        return (0.0, 0.0);
    }
    let p = w / n as f64;
    (p, 1.96 * (p * (1.0 - p) / n as f64).sqrt())
}

/// Win rate of the bot called `name` over games between two different bots.
fn rate_of(rs: &[GameResult], name: &str) -> Rate {
    let games: Vec<&GameResult> = rs.iter().filter(|r| r.names[0] != r.names[1]).collect();
    let wins = games.iter().filter(|r| r.names[r.winner] == name).count();
    let (p, ci) = rate_ci(wins as f64, games.len());
    Rate { p, ci, n: games.len() }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// 95% CI half-width of a mean.
fn mean_ci(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    if n < 2.0 {
        return f64::NAN;
    }
    let m = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0);
    1.96 * (var / n).sqrt()
}

fn pc(x: f64) -> String {
    if x.is_finite() {
        format!("{:.1}%", x * 100.0)
    } else {
        "n/a".into()
    }
}

/// The leader after each round (`None` while level), from the core-HP trajectory.
fn leaders(r: &GameResult) -> impl Iterator<Item = Option<usize>> + '_ {
    r.hp_traj.iter().map(|hp| (hp[0] != hp[1]).then(|| usize::from(hp[1] > hp[0])))
}

fn lead_changes(r: &GameResult) -> f64 {
    let mut n = 0.0;
    let mut leader = None;
    for l in leaders(r).flatten() {
        if leader.is_some_and(|x| x != l) {
            n += 1.0;
        }
        leader = Some(l);
    }
    n
}

/// Everything the reports read from a set of games.
struct Pooled {
    n: usize,
    seat_a: f64,
    median_rounds: f64,
    reach_r5: f64,
    median_first_dmg: f64,
    comeback: f64,
    comeback_n: usize,
    comeback_ci: f64,
    hp_decided: f64,
    close_finish: f64,
    wire_to_wire: f64,
    live_last_round: f64,
    lead_changes: f64,
    lead_changes_ci: f64,
    tp_share: [f64; 7],
    count_share: [f64; 7],
    reverted_rate: f64,
    tp_use: f64,
    nohelp_eligible: f64,
    nohelp_share: f64,
    scars_per_game: f64,
    scar_round_share: f64,
    draft_collision_share: f64,
    /// Share of rounds (from round 2) whose crossing equals the previous round's.
    cross_repeat: f64,
    cross_rounds: f64,
    cmd_pts: [f64; 4],
    cmd_n: [usize; 4],
    /// Per side and round: legal (tile, op) choices, ops played, buildings placed, units bought.
    per_round: [f64; 4],
    probe: Probe,
}

fn pooled(rs: &[GameResult]) -> Pooled {
    let n = rs.len();
    let nf = n.max(1) as f64;
    let share = |f: &dyn Fn(&GameResult) -> bool| rs.iter().filter(|r| f(r)).count() as f64 / nf;
    let (mut cb_n, mut cb_w) = (0, 0);
    for r in rs {
        if let Some(hp) = r.hp_r2.filter(|hp| hp[0] != hp[1]) {
            cb_n += 1;
            cb_w += usize::from(r.winner == usize::from(hp[1] < hp[0]));
        }
    }
    let (mut tp, mut ct) = ([0f64; 7], [0f64; 7]);
    let (mut reverted, mut spent, mut avail) = (0f64, 0f64, 0f64);
    let (mut nh_elig, mut nh_none) = (0f64, 0f64);
    let (mut cross_rep, mut cross_n) = (0f64, 0f64);
    let (mut cmd_pts, mut cmd_n) = ([0f64; 4], [0usize; 4]);
    let mut per_round = [0f64; 4];
    let mut probe = Probe::default();
    for r in rs {
        probe.add(&r.probe);
        for s in 0..2 {
            cross_rep += r.cross_repeat[s] as f64;
            cross_n += r.cross_rounds[s] as f64;
            nh_elig += r.nohelp_eligible[s] as f64;
            nh_none += r.nohelp_none[s] as f64;
            cmd_pts[r.commanders[s]] += f64::from(r.winner == s);
            cmd_n[r.commanders[s]] += 1;
            for o in 0..7 {
                tp[o] += r.tp_by_op[s][o] as f64;
                ct[o] += r.count_by_op[s][o] as f64;
            }
            reverted += r.reverted[s] as f64;
            spent += r.tp_spent[s] as f64;
            avail += r.tp_avail[s] as f64;
            per_round[0] += r.legal_ops[s] as f64;
            per_round[1] += r.count_by_op[s].iter().sum::<i32>() as f64;
            per_round[2] += r.built[s] as f64;
            per_round[3] += r.bought[s] as f64;
        }
    }
    let rounds: f64 = rs.iter().map(|r| r.rounds as f64).sum::<f64>().max(1.0);
    let (tsum, csum) = (tp.iter().sum::<f64>().max(1.0), ct.iter().sum::<f64>().max(1.0));
    let changes: Vec<f64> = rs.iter().map(lead_changes).collect();
    Pooled {
        n,
        seat_a: share(&|r| r.winner == 0),
        median_rounds: median(rs.iter().map(|r| r.rounds as f64).collect()),
        reach_r5: share(&|r| r.rounds >= 5),
        median_first_dmg: median(rs.iter().map(|r| r.first_dmg.map_or(6.0, |x| x as f64)).collect()),
        comeback: if cb_n > 0 { cb_w as f64 / cb_n as f64 } else { f64::NAN },
        comeback_n: cb_n,
        comeback_ci: rate_ci(cb_w as f64, cb_n).1,
        hp_decided: share(&|r| r.hp_traj.last().is_some_and(|h| h[0] > 0 && h[1] > 0)),
        close_finish: share(&|r| r.hp_traj.last().is_some_and(|h| (h[0] - h[1]).abs() <= 4)),
        // the winner led after every round
        wire_to_wire: share(&|r| !r.hp_traj.is_empty() && leaders(r).all(|l| l == Some(r.winner))),
        live_last_round: share(&|r| {
            let t = &r.hp_traj;
            let before_last = if t.len() >= 2 { t[t.len() - 2] } else { [20, 20] };
            (before_last[0] - before_last[1]).abs() <= 4
        }),
        lead_changes: changes.iter().sum::<f64>() / nf,
        lead_changes_ci: mean_ci(&changes),
        tp_share: tp.map(|x| x / tsum),
        count_share: ct.map(|x| x / csum),
        reverted_rate: reverted / csum,
        tp_use: spent / avail.max(1.0),
        nohelp_eligible: nh_elig,
        nohelp_share: if nh_elig > 0.0 { nh_none / nh_elig } else { f64::NAN },
        scars_per_game: rs.iter().map(|r| r.scars as f64).sum::<f64>() / nf,
        scar_round_share: rs.iter().map(|r| r.scar_rounds as f64).sum::<f64>() / rounds,
        draft_collision_share: rs.iter().map(|r| r.draft_collisions as f64).sum::<f64>() / rounds,
        cross_repeat: if cross_n > 0.0 { cross_rep / cross_n } else { f64::NAN },
        cross_rounds: cross_n,
        cmd_pts,
        cmd_n,
        per_round: per_round.map(|x| x / (2.0 * rounds)),
        probe,
    }
}

fn dead_ops(share: &[f64; 7]) -> Vec<&'static str> {
    OPS.iter().zip(share.iter()).filter(|(_, s)| **s < 0.02).map(|(n, _)| *n).collect()
}

struct DealStats {
    deals: usize,
    replays: usize,
    between: f64,
    within: f64,
    share: f64,
    lopsided: f64,
    seat_a: f64,
}

/// For each deal (a game seed: map, commanders, offers), `replays` games of `search` vs `search` that differ only in
/// the bots' RNG seeds; no seat swap. The outcome is seat A's points.
fn deal_stats(deals: usize, replays: usize, base_seed: u64, threads: usize) -> DealStats {
    let o: Vec<Vec<f64>> = par_indexed(deals, threads, |i| {
        let deal = base_seed + i as u64;
        (0..replays as u64)
            .map(|k| {
                let mut rng = Rng::seed_from_u64(deal.wrapping_mul(1_000_003).wrapping_add(k + 1));
                let mut a = make_bot("search", rng.next_u64());
                let mut b = make_bot("search", rng.next_u64());
                f64::from(play(deal, a.as_mut(), b.as_mut(), false, Instr::Off).0.winner == 0)
            })
            .collect::<Vec<f64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    let (between, within, share) = probes::variance_share(&o);
    DealStats {
        deals,
        replays,
        between,
        within,
        share,
        lopsided: probes::lopsided_share(&o),
        seat_a: o.iter().flatten().sum::<f64>() / (o.len() * replays).max(1) as f64,
    }
}

fn style_label() -> String {
    std::env::var("MT_STYLE").unwrap_or_else(|_| "own".into())
}

fn play_commanders(seed: u64, cmds: [Commander; 2]) -> (usize, [Vec<Card>; 2], [Vec<Card>; 2]) {
    let mut g = Game::new_with_commanders(seed, cmds);
    let mut a = make_bot("search", seed.wrapping_mul(7919) + 1);
    let mut b = make_bot("search", seed.wrapping_mul(7919) + 2);
    let mut offered = [Vec::new(), Vec::new()];
    while !g.is_over() {
        g.start_round();
        for o in offered.iter_mut() {
            o.extend_from_slice(&g.offers);
        }
        draft_round(&mut g, a.as_mut(), b.as_mut());
        mt_bots::play_round_after_draft(&mut g, a.as_mut(), b.as_mut());
    }
    let picked = [g.players[0].picks[1..].to_vec(), g.players[1].picks[1..].to_vec()];
    (g.winner(), offered, picked)
}

fn commanders(games: usize, base_seed: u64) {
    let threads = thread_count();
    let t0 = Instant::now();
    let pairs: Vec<(usize, usize)> = (0..4).flat_map(|i| (i + 1..4).map(move |j| (i, j))).collect();
    let jobs: Vec<(usize, u64, bool)> = (0..pairs.len())
        .flat_map(|p| (0..games as u64 / 2).flat_map(move |s| [(p, base_seed + s, false), (p, base_seed + s, true)]))
        .collect();
    let results: Vec<_> = par_indexed(jobs.len(), threads, |k| {
        let (p, seed, swap) = jobs[k];
        let (i, j) = pairs[p];
        let cmds = if swap {
            [ALL_COMMANDERS[j], ALL_COMMANDERS[i]]
        } else {
            [ALL_COMMANDERS[i], ALL_COMMANDERS[j]]
        };
        (cmds, play_commanders(seed, cmds))
    })
    .into_iter()
    .flatten()
    .collect();

    let mut pts = [[0f64; 4]; 4];
    let mut n = [[0usize; 4]; 4];
    // per card: offered, picked, games picked, points when picked
    let mut cards: std::collections::BTreeMap<&str, (usize, usize, usize, f64)> = CARD_POOL.iter().map(|c| (c.name(), (0, 0, 0, 0.0))).collect();
    for (cmds, (w, offered, picked)) in &results {
        for side in 0..2 {
            let (me, op) = (cmds[side] as usize, cmds[1 - side] as usize);
            let won = f64::from(*w == side);
            pts[me][op] += won;
            n[me][op] += 1;
            for c in &offered[side] {
                cards.entry(c.name()).or_default().0 += 1;
            }
            for c in &picked[side] {
                cards.entry(c.name()).or_default().1 += 1;
            }
            let mut names: Vec<&str> = picked[side].iter().map(|c| c.name()).collect();
            names.sort();
            names.dedup();
            for name in names {
                let e = cards.entry(name).or_default();
                e.2 += 1;
                e.3 += won;
            }
        }
    }

    let names = ALL_COMMANDERS.map(|c| c.name());
    println!("# Commander matchups (v{VERSION})\n");
    println!(
        "Search vs search with forced commanders, bot style `{}`. {games} games per pair, each seed played with both seat \
         assignments (seeds {base_seed}..{}). Mirror matches aren't dealt in real games, so the diagonal is empty.\n",
        style_label(),
        base_seed + games as u64 / 2
    );
    println!("## Win rate of the row commander against the column commander\n");
    println!("| | {} | Overall |", names.join(" | "));
    println!("| --- |{}", " --- |".repeat(5));
    let mut flags = Vec::new();
    for i in 0..4 {
        let mut row = format!("| **{}** |", names[i]);
        for j in 0..4 {
            if i == j {
                row += " - |";
                continue;
            }
            let (p, ci) = rate_ci(pts[i][j], n[i][j]);
            row += &format!(" {:.1}% \u{b1}{:.1} |", p * 100.0, ci * 100.0);
            if i < j && !(0.40..=0.60).contains(&p) {
                flags.push(format!("{} vs {}: {:.1}% (outside 40-60%)", names[i], names[j], p * 100.0));
            }
        }
        let (tp, tn): (f64, usize) = (0..4).fold((0.0, 0), |(a, b), j| (a + pts[i][j], b + n[i][j]));
        let (p, ci) = rate_ci(tp, tn);
        row += &format!(" {:.1}% \u{b1}{:.1} |", p * 100.0, ci * 100.0);
        println!("{row}");
    }

    println!("\n## Cards (drafted picks only; commander start cards excluded)\n");
    println!("Pick rate = picked / offered. Win when picked = points per game for a side that drafted the card at least once.");
    println!("Win when picked is biased by who drafts what, so read it as a pointer, not a balance verdict.\n");
    println!("| Card | Offered | Picked | Pick rate | Win when picked | Games picked |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for (name, (off, pick, gp, wp)) in &cards {
        let pr = *pick as f64 / (*off).max(1) as f64;
        let (wr, ci) = rate_ci(*wp, *gp);
        let wr_s = if *gp == 0 {
            "-".to_string()
        } else {
            format!("{:.1}% \u{b1}{:.1}", wr * 100.0, ci * 100.0)
        };
        println!("| {name} | {off} | {pick} | {:.1}% | {wr_s} | {gp} |", pr * 100.0);
        if pr < 0.05 {
            flags.push(format!("{name}: pick rate {:.1}% (under 5%)", pr * 100.0));
        }
        if *gp > 0 && wr > 0.60 {
            flags.push(format!("{name}: win when picked {:.1}% (over 60%)", wr * 100.0));
        }
    }
    println!("\n## Flags\n");
    if flags.is_empty() {
        println!("None.");
    }
    for f in &flags {
        println!("- {f}");
    }
    eprintln!("commanders: {} games in {:.1}s", results.len(), t0.elapsed().as_secs_f64());
}

/// Same bot and bot seed in both seats of a mirrored start: every decision must mirror and core HP must stay level.
/// Returns the number of seeds that diverged before the last round.
fn symcheck(name: &str, seeds: u64) -> u32 {
    let mut early = 0;
    for seed in 0..seeds {
        let mut g = Game::new(seed);
        g.players[1] = g.players[0].clone();
        let mut a = make_bot(name, 77);
        let mut b = make_bot(name, 77);
        while !g.is_over() {
            g.start_round();
            let (da, db) = (a.draft(&g, 0), b.draft(&g, 1));
            let (pa, pb) = (a.terraform(&g, 0), b.terraform(&g, 1));
            let (ba, bb) = (a.build(&g.shape_view(0, &pa), 0), b.build(&g.shape_view(1, &pb), 1));
            let sorted = |mut v: Vec<(usize, usize)>| {
                v.sort();
                v
            };
            let plans_differ = sorted(pa.iter().map(|&(t, o)| (mirror(t), o as usize)).collect()) != sorted(pb.iter().map(|&(t, o)| (t, o as usize)).collect());
            let builds_differ = ba.units != bb.units
                || sorted(ba.buildings.iter().map(|&(k, t)| (k as usize, mirror(t))).collect())
                    != sorted(bb.buildings.iter().map(|&(k, t)| (k as usize, t)).collect());
            let mut diverged = None;
            if da != db {
                diverged = Some(format!("draft orders differ\n  A {da:?}\n  B {db:?}"));
            } else if plans_differ {
                diverged = Some(format!("plans differ\n  A {pa:?}\n  B {pb:?}"));
            } else if builds_differ {
                diverged = Some(format!("builds differ\n  A {ba:?}\n  B {bb:?}"));
            }
            if diverged.is_none() {
                g.apply_draft([da, db]);
                g.apply_terraform([pa, pb]);
                g.apply_build([ba, bb]);
                let [ca, cb] = attacks(&g, a.as_mut(), b.as_mut());
                if ca != cb {
                    diverged = Some(format!("crossings differ\n  A {ca}\n  B {cb}"));
                } else {
                    g.apply_attack([ca, cb]);
                    g.clash_and_resolve();
                    if g.players[0].core_hp != g.players[1].core_hp {
                        diverged = Some(format!("hp {} vs {}", g.players[0].core_hp, g.players[1].core_hp));
                    }
                }
            }
            if let Some(why) = diverged {
                println!("seed {seed} round {}: {why}", g.round);
                early += u32::from(g.round < MAX_ROUNDS);
                break;
            }
        }
    }
    early
}

/// Same seed twice gives the same game, and instrumentation changes nothing. Returns false on any mismatch.
fn verify(seed: u64) -> bool {
    let go = |x: &str, y: &str, instr| {
        let mut a = make_bot(x, 11);
        let mut b = make_bot(y, 12);
        play(seed, a.as_mut(), b.as_mut(), false, instr).0
    };
    let (h1, h2) = (go("search", "greedy", Instr::Off).hash, go("search", "greedy", Instr::Off).hash);
    println!("hash {h1:016x} vs {h2:016x}: {}", if h1 == h2 { "deterministic" } else { "MISMATCH" });
    let mut ok = h1 == h2;
    for (x, y, instr) in [
        ("search", "search", Instr::Full),
        ("search", "greedy", Instr::Full),
        ("shaper", "shaper", Instr::Cheap),
        ("shaper", "rand_shape", Instr::Full),
    ] {
        let (off, on) = (go(x, y, Instr::Off), go(x, y, instr));
        let same = off.hash == on.hash && off.winner == on.winner && off.hp_traj == on.hp_traj && off.tp_by_op == on.tp_by_op;
        println!(
            "probes {instr:?} {x} vs {y}: hash {:016x} off, {:016x} on: {}; counterfactual self-check failures {}",
            off.hash,
            on.hash,
            if same { "identical" } else { "MISMATCH" },
            on.probe.selfcheck_fail
        );
        ok &= same && on.probe.selfcheck_fail == 0;
    }
    ok
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize, d: &str| args.get(i).cloned().unwrap_or_else(|| d.to_string());
    let num = |i: usize, d: u64| arg(i, "").parse::<u64>().unwrap_or(d);
    match arg(1, "help").as_str() {
        "match" => {
            let (x, y) = (arg(2, "search"), arg(3, "greedy"));
            let (games, seed) = (num(4, 100) as usize, num(5, 1));
            let t = Instant::now();
            let rs = head_to_head(&x, &y, games, seed, thread_count(), Instr::Off);
            let pl = pooled(&rs);
            if x == y {
                println!(
                    "{x} mirror: seat A wins {:.1}% \u{b1}{:.1} over {} games | median rounds {:.0} | {:.1}s",
                    pl.seat_a * 100.0,
                    196.0 * (0.25 / pl.n.max(1) as f64).sqrt(),
                    pl.n,
                    pl.median_rounds,
                    t.elapsed().as_secs_f64()
                );
                return;
            }
            let r = rate_of(&rs, &x);
            println!(
                "{x} vs {y}: {:.1}% \u{b1}{:.1} over {} games | median rounds {:.0}, seat A {:.1}% | {:.1}s",
                r.p * 100.0,
                r.ci * 100.0,
                r.n,
                pl.median_rounds,
                pl.seat_a * 100.0,
                t.elapsed().as_secs_f64()
            );
        }
        "suite" => scorecard::suite(&arg(2, "results/try"), num(3, 400) as usize),
        "quick" => println!("{}", scorecard::measure(num(2, 400) as usize).json()),
        "symcheck" => {
            let early = symcheck(&arg(2, "search"), num(3, 20));
            println!("symcheck: {early} divergence(s) before round {MAX_ROUNDS}");
            if early > 0 {
                std::process::exit(1);
            }
        }
        "selfplay" => {
            let (games, seed, bot) = (num(2, 400) as usize, num(3, 1), arg(4, "search"));
            let t = Instant::now();
            let sp = pooled(&head_to_head(&bot, &bot, games, seed, thread_count(), Instr::Cheap));
            let mut j = scorecard::selfplay_json(&sp);
            j["version"] = VERSION.into();
            j["bot"] = bot.into();
            j["seed"] = seed.into();
            j["secs"] = ((t.elapsed().as_secs_f64() * 10.0).round() / 10.0).into();
            println!("{j}");
        }
        "deals" => {
            let (deals, replays, seed) = (num(2, 200) as usize, num(3, 10) as usize, num(4, 1));
            let t = Instant::now();
            let d = deal_stats(deals, replays, seed, thread_count());
            println!(
                "deals: {deals} deals x {replays} replays (search vs search, seeds {seed}..{}, v{VERSION}), {} games\n\
                 outcome = seat A points (1 / 0); seat A mean {:.3}\n\
                 between-deal variance {:.5}, within-deal variance {:.5}\n\
                 variance share fixed by the deal: {:.1}% (C4 target <= 15%)\n\
                 deals where one seat scores >= 90%: {:.1}%\n\
                 {:.1}s",
                seed + deals as u64,
                deals * replays,
                d.seat_a,
                d.between,
                d.within,
                d.share * 100.0,
                d.lopsided * 100.0,
                t.elapsed().as_secs_f64()
            );
        }
        "commanders" => commanders(num(2, 400) as usize, num(3, 1)),
        "replay" => {
            let seed = num(4, 1);
            let mut a = make_bot(&arg(2, "search"), seed * 3 + 1);
            let mut b = make_bot(&arg(3, "search"), seed * 3 + 2);
            let (r, json) = play(seed, a.as_mut(), b.as_mut(), true, Instr::Off);
            std::fs::write(arg(5, "replay.json"), json.unwrap_or_default()).ok();
            println!("winner seat {} after {} rounds", r.winner, r.rounds);
        }
        "verify" => {
            if !verify(num(2, 1)) {
                std::process::exit(1);
            }
        }
        _ => {
            println!(
                "mt_sim: Mirror Terrain headless simulator (rules v{VERSION})\n\n\
                 MT_THREADS=<n> sets the worker thread count (default: all cores).\n\
                 MT_STYLE=<balanced|rush|turtle|air|ground> gives every commander the same bot style.\n\
                 MT_DEBUG=1 makes the search bot print its payoff matrices to stderr.\n\n\
                 commands:\n\
                 \x20 match <botA> <botB> [games=100] [seed=1]   seat-swapped head-to-head\n\
                 \x20 suite [outdir=results/try] [games=400]      the scorecard (criteria C1-C11 + match checks) as report.md and report.json, plus replays\n\
                 \x20 quick [games=400]                            the same scorecard as one JSON line, no files\n\
                 \x20 replay <botA> <botB> [seed=1] [out.json]     record one game for the viewer\n\
                 \x20 verify [seed=1]                              same seed twice -> same hash; probes on vs off -> same game (exit 1 if not)\n\
                 \x20 symcheck [bot=search] [seeds=20]             same bot+seed in both seats must mirror (exit 1 before round 5)\n\
                 \x20 commanders [games=400] [seed=1]              commander win-rate matrix + card pick stats (Markdown)\n\
                 \x20 selfplay [games=400] [seed=1] [bot=search]   pooled self-play metrics as JSON\n\
                 \x20 deals [deals=200] [replays=10] [seed=1]      share of outcome variance fixed by the deal (search vs search, C4)\n\n\
                 bots: {}",
                BOT_NAMES.join(" ")
            );
        }
    }
}
