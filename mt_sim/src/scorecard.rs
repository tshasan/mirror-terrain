//! The scorecard: criteria C1-C11 of `docs/SPEC.md` plus the match checks of `docs/METRICS.md`, measured once and
//! rendered as Markdown (`report.md`) and JSON (`report.json`, the `quick` line).

use super::*;
use serde_json::{json, Value};
use std::fmt::Write as _;

/// Base seeds, one range per matchup so no two measurements share games.
mod seeds {
    pub const GREEDY_VS_RANDOM: u64 = 10000;
    pub const SEARCH_VS_GREEDY: u64 = 3000;
    pub const SHAPER_VS_SEARCH: u64 = 45000;
    pub const C1: u64 = 41000;
    pub const C1_OPS: u64 = 42000;
    pub const C1_PLACE: u64 = 43000;
    pub const C2: u64 = 44000;
    pub const C3: u64 = 47000;
    pub const SEARCH_VS_RANDCROSS: u64 = 12000;
    pub const C6: u64 = 49000;
    pub const C7: u64 = 48000;
    pub const RUSH_VS_FORT: u64 = 4100;
    pub const RUSH_VS_TURTLE: u64 = 4000;
    pub const COUNTER_VS_RUSH: u64 = 7000;
    pub const FORT_VS_SHAPER: u64 = 4200;
    pub const FORT_VS_SEARCH: u64 = 4300;
    pub const SEARCH_SELFPLAY: u64 = 5000;
    pub const SHAPER_SELFPLAY: u64 = 46000;
    pub const DEALS: u64 = 30000;
}

/// Every head-to-head the scorecard plays: (key, first bot, second bot, base seed). The rate is the first bot's.
const MATCHUPS: [(&str, &str, &str, u64); 16] = [
    ("greedy_vs_random", "greedy", "random", seeds::GREEDY_VS_RANDOM),
    ("search_vs_greedy", "search", "greedy", seeds::SEARCH_VS_GREEDY),
    ("shaper_vs_search", "shaper", "search", seeds::SHAPER_VS_SEARCH),
    ("c1_shaper_vs_no_shape", "shaper", "no_shape", seeds::C1),
    ("c1_shape_only_vs_no_shape", "shape_only", "no_shape", seeds::C1_OPS),
    ("c1_place_only_vs_no_shape", "place_only", "no_shape", seeds::C1_PLACE),
    ("c2_shaper_vs_rand_shape", "shaper", "rand_shape", seeds::C2),
    ("c3_shaper_vs_randcross", "shaper", "shaper_randcross", seeds::C3),
    ("search_vs_randcross", "search", "search_randcross", seeds::SEARCH_VS_RANDCROSS),
    ("c6_shaper_vs_scarblind", "shaper", "shaper_scarblind", seeds::C6),
    ("c7_shaper_vs_fixed", "shaper", "shaper_fixed", seeds::C7),
    ("c8_rush_vs_fort", "persona_rush", "persona_fort", seeds::RUSH_VS_FORT),
    ("c8_counter_vs_rush", "persona_counter", "persona_rush", seeds::COUNTER_VS_RUSH),
    ("rush_vs_turtle", "persona_rush", "persona_turtle", seeds::RUSH_VS_TURTLE),
    ("fort_vs_shaper", "persona_fort", "shaper", seeds::FORT_VS_SHAPER),
    ("fort_vs_search", "persona_fort", "search", seeds::FORT_VS_SEARCH),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    /// The simulator cannot measure it yet; the note says what is missing.
    Pending,
    /// Measured, no bar.
    Reported,
}

impl Status {
    fn of(ok: bool) -> Status {
        if ok {
            Status::Pass
        } else {
            Status::Fail
        }
    }
    fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Pending => "pending",
            Status::Reported => "reported",
        }
    }
    fn md(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Fail => "**FAIL**",
            Status::Pending => "pending",
            Status::Reported => "reported",
        }
    }
}

struct Criterion {
    id: &'static str,
    claim: &'static str,
    measure: &'static str,
    value: String,
    target: &'static str,
    status: Status,
    note: String,
}

struct Check {
    name: &'static str,
    value: String,
    bar: String,
    ok: bool,
}

/// A feel metric with a banded bar (C10).
struct Feel {
    name: &'static str,
    key: &'static str,
    value: f64,
    ci: f64,
    band: (f64, f64),
    pct: bool,
}

impl Feel {
    fn outside(&self) -> f64 {
        (self.band.0 - self.value).max(self.value - self.band.1).max(0.0)
    }
    /// Outside its bar by more than its own CI. A value outside the bar but within the CI of it is a re-run, not a failure.
    fn fails(&self) -> bool {
        self.outside() > self.ci
    }
    fn show(&self) -> String {
        if self.pct {
            format!("{:.1}% \u{b1}{:.1}", self.value * 100.0, self.ci * 100.0)
        } else {
            format!("{:.2} \u{b1}{:.2}", self.value, self.ci)
        }
    }
    fn bar(&self) -> String {
        if self.pct {
            format!("{:.0}-{:.0}%", self.band.0 * 100.0, self.band.1 * 100.0)
        } else {
            format!("{:.1}-{:.1}", self.band.0, self.band.1)
        }
    }
    fn verdict(&self) -> &'static str {
        if self.fails() {
            "**outside the bar**"
        } else if self.outside() > 0.0 {
            "outside the bar, within its CI"
        } else {
            "ok"
        }
    }
}

pub struct Scorecard {
    games: usize,
    secs: f64,
    rates: Vec<(&'static str, &'static str, &'static str, Rate)>,
    search_sp: Pooled,
    shaper_sp: Pooled,
    deals: DealStats,
}

fn pm(r: Rate) -> String {
    format!("{:.1}% \u{b1}{:.1}", r.p * 100.0, r.ci * 100.0)
}

/// A reminder when the bar sits inside the CI: the verdict could flip on another seed range.
fn bar_note(r: Rate, bar: f64) -> String {
    if (r.p - bar).abs() < r.ci {
        format!("the {:.0}% bar is inside the CI; re-run on another seed range", bar * 100.0)
    } else {
        String::new()
    }
}

fn r4(x: f64) -> Value {
    json!((x * 1e4).round() / 1e4)
}

pub fn selfplay_json(p: &Pooled) -> Value {
    json!({
        "games": p.n,
        "seat_a": r4(p.seat_a),
        "median_rounds": p.median_rounds,
        "reach_r5": r4(p.reach_r5),
        "reach_cap": r4(p.reach_cap),
        "rounds_hist": p.rounds_hist.iter().map(|x| r4(*x)).collect::<Vec<_>>(),
        "median_first_dmg": p.median_first_dmg,
        "hp_decided": r4(p.hp_decided),
        "knockout": r4(1.0 - p.hp_decided),
        "comeback": r4(p.comeback),
        "comeback_n": p.comeback_n,
        "comeback_ci": r4(p.comeback_ci),
        "close_finish": r4(p.close_finish),
        "wire_to_wire": r4(p.wire_to_wire),
        "lead_changes": r4(p.lead_changes),
        "lead_changes_ci": r4(p.lead_changes_ci),
        "live_last_round": r4(p.live_last_round),
        "tp_share": p.tp_share.map(r4),
        "count_share": p.count_share.map(r4),
        "dead_ops": dead_ops(&p.count_share),
        "tp_use": r4(p.tp_use),
        "reverted_rate": r4(p.reverted_rate),
        "nohelp_share": r4(p.nohelp_share),
        "nohelp_decisions": p.nohelp_eligible,
        "scars_per_game": r4(p.scars_per_game),
        "scar_round_share": r4(p.scar_round_share),
        "draft_collision_share": r4(p.draft_collision_share),
        "cross_repeat_share": r4(p.cross_repeat),
        "cross_repeat_rounds": p.cross_rounds,
    })
}

/// Play everything the scorecard needs: `games` games per matchup, seat-swapped.
pub fn measure(games: usize) -> Scorecard {
    let t0 = Instant::now();
    let threads = thread_count();
    let timed = |label: &str, t: Instant| eprintln!("{label}: {:.1}s", t.elapsed().as_secs_f64());
    let rates = MATCHUPS
        .iter()
        .map(|&(key, x, y, seed)| {
            let t = Instant::now();
            let r = rate_of(&head_to_head(x, y, games, seed, threads, Instr::Off), x);
            timed(key, t);
            (key, x, y, r)
        })
        .collect();
    let t = Instant::now();
    let search_sp = pooled(&head_to_head("search", "search", games, seeds::SEARCH_SELFPLAY, threads, Instr::Full));
    timed("search_selfplay (with probes)", t);
    let t = Instant::now();
    let shaper_sp = pooled(&head_to_head("shaper", "shaper", games, seeds::SHAPER_SELFPLAY, threads, Instr::Cheap));
    timed("shaper_selfplay", t);
    let t = Instant::now();
    let deals = deal_stats((games / 2).max(2), 10, seeds::DEALS, threads);
    timed("deals", t);
    Scorecard {
        games,
        secs: t0.elapsed().as_secs_f64(),
        rates,
        search_sp,
        shaper_sp,
        deals,
    }
}

impl Scorecard {
    fn rate(&self, key: &str) -> Rate {
        self.rates.iter().find(|r| r.0 == key).map(|r| r.3).expect("a matchup key from MATCHUPS")
    }

    fn mean(sum: f64, n: i32) -> f64 {
        if n > 0 {
            sum / n as f64
        } else {
            f64::NAN
        }
    }

    fn guess_share(&self) -> f64 {
        let p = &self.search_sp.probe;
        Self::mean(p.guess_rounds as f64, p.rounds)
    }

    fn ladder(&self) -> [Rate; 3] {
        [self.rate("greedy_vs_random"), self.rate("search_vs_greedy"), self.rate("shaper_vs_search")]
    }

    fn feels(&self) -> Vec<Feel> {
        let sp = &self.search_sp;
        let rate = |name, key, value: f64, band| Feel {
            name,
            key,
            value,
            ci: rate_ci(value * sp.n as f64, sp.n).1,
            band,
            pct: true,
        };
        vec![
            rate("Close finish (final core margin \u{2264} 4)", "close_finish", sp.close_finish, (0.30, 0.50)),
            rate("Wire-to-wire wins", "wire_to_wire", sp.wire_to_wire, (0.15, 0.40)),
            Feel {
                name: "Lead changes per game",
                key: "lead_changes",
                value: sp.lead_changes,
                ci: sp.lead_changes_ci,
                band: (0.4, 1.0),
                pct: false,
            },
            Feel {
                name: "Comeback (behind after R2, still wins)",
                key: "comeback",
                value: sp.comeback,
                ci: sp.comeback_ci,
                band: (0.20, 0.35),
                pct: true,
            },
        ]
    }

    /// Decisions one player makes in a round of `shaper` self-play: draft + ops + buildings + units + crossing.
    fn decisions_per_round(&self) -> f64 {
        let [_, ops, built, bought] = self.shaper_sp.per_round;
        2.0 + ops + built + bought
    }

    fn criteria(&self) -> Vec<Criterion> {
        let (sp, sh, pr) = (&self.search_sp, &self.shaper_sp, &self.search_sp.probe);
        let (c1, c2, c3, c6, c7) = (
            self.rate("c1_shaper_vs_no_shape"),
            self.rate("c2_shaper_vs_rand_shape"),
            self.rate("c3_shaper_vs_randcross"),
            self.rate("c6_shaper_vs_scarblind"),
            self.rate("c7_shaper_vs_fixed"),
        );
        let (rush_fort, counter_rush) = (self.rate("c8_rush_vs_fort"), self.rate("c8_counter_vs_rush"));
        let guess = self.guess_share();
        let ladder = self.ladder();
        let steps = ladder.iter().filter(|r| r.p >= 0.60).count();
        let feels = self.feels();
        let feel_bad: Vec<&str> = feels.iter().filter(|f| f.fails()).map(|f| f.key).collect();
        vec![
            Criterion {
                id: "C1",
                claim: "The base is the hook",
                measure: "`shaper` vs `no_shape`",
                value: pm(c1),
                target: "\u{2265} 60%",
                status: Status::of(c1.p >= 0.60),
                note: bar_note(c1, 0.60),
            },
            Criterion {
                id: "C2",
                claim: "Good shaping beats any shaping",
                measure: "`shaper` vs `rand_shape`",
                value: pm(c2),
                target: "\u{2265} 60%",
                status: Status::of(c2.p >= 0.60),
                note: bar_note(c2, 0.60),
            },
            Criterion {
                id: "C3",
                claim: "The attack is a read",
                measure: "`shaper` vs `shaper_randcross` (random crossing); rounds with no pure crossing equilibrium (`search` self-play)",
                value: format!("{} and {}", pm(c3), pc(guess)),
                target: "\u{2265} 60% and \u{2264} 10%",
                status: Status::of(c3.p >= 0.60 && guess <= 0.10),
                note: format!(
                    "`search` vs `search_randcross` {}; guess gap {:.2} eval units; big guesses (\u{2265} 4 core HP) {} of rounds; interaction share {}",
                    pm(self.rate("search_vs_randcross")),
                    pr.gap_sum / pr.rounds.max(1) as f64,
                    pc(Self::mean(pr.big_guess_rounds as f64, pr.rounds)),
                    pc(if pr.total_ss > 0.0 { pr.inter_ss / pr.total_ss } else { f64::NAN }),
                ),
            },
            Criterion {
                id: "C4",
                claim: "Players decide the game",
                measure: "share of outcome variance fixed by the deal",
                value: pc(self.deals.share),
                target: "\u{2264} 15%",
                status: Status::of(self.deals.share <= 0.15),
                note: format!(
                    "{} deals x {} replays; {} of deals go 90%+ to one seat; first choices collide in {} of drafts",
                    self.deals.deals,
                    self.deals.replays,
                    pc(self.deals.lopsided),
                    pc(sp.draft_collision_share)
                ),
            },
            Criterion {
                id: "C5",
                claim: "There is depth",
                measure: "ladder steps won by \u{2265} 60% (greedy > random, search > greedy, shaper > search)",
                value: format!("{steps} of 3 ({}, {}, {})", pm(ladder[0]), pm(ladder[1]), pm(ladder[2])),
                target: "\u{2265} 4",
                status: if steps < 3 { Status::Fail } else { Status::Pending },
                note: "the ladder has three rungs; the fourth (a two-round lookahead) is not built".into(),
            },
            Criterion {
                id: "C6",
                claim: "Scars follow from decisions",
                measure: "`shaper` vs `shaper_scarblind` (the same bot deciding on the board without its scars)",
                value: pm(c6),
                target: "\u{2265} 55%",
                status: Status::of(c6.p >= 0.55),
                note: bar_note(c6, 0.55),
            },
            Criterion {
                id: "C6b",
                claim: "Scars happen, but not everywhere",
                measure: "scars per game; rounds that leave at least one (`shaper` self-play)",
                value: format!("{:.1} and {}", sh.scars_per_game, pc(sh.scar_round_share)),
                target: "2-6; 40-80%",
                status: Status::of((2.0..=6.0).contains(&sh.scars_per_game) && (0.40..=0.80).contains(&sh.scar_round_share)),
                note: format!("`search` self-play: {:.1} and {}", sp.scars_per_game, pc(sp.scar_round_share)),
            },
            Criterion {
                id: "C6c",
                claim: "Scars change play",
                measure: "share of crossings that repeat last round's column",
                value: format!("{} (`shaper`), {} (`search`)", pc(sh.cross_repeat), pc(sp.cross_repeat)),
                target: "lower with scars than without",
                status: Status::Reported,
                note: "scars are part of the rules, so there is no scars-off run to compare with".into(),
            },
            Criterion {
                id: "C7",
                claim: "It is not solitaire",
                measure: "`shaper` vs `shaper_fixed` (units and building kinds from `default_build`, blind to the opponent)",
                value: pm(c7),
                target: "\u{2265} 55%",
                status: Status::of(c7.p >= 0.55),
                note: bar_note(c7, 0.55),
            },
            Criterion {
                id: "C8",
                claim: "Defending is viable but not dominant",
                measure: "`persona_rush` vs `persona_fort`; `persona_counter` vs `persona_rush`",
                value: format!("{} and {}", pm(rush_fort), pm(counter_rush)),
                target: "\u{2264} 65%; 40-60%",
                status: Status::of(rush_fort.p <= 0.65 && (0.40..=0.60).contains(&counter_rush.p)),
                note: format!(
                    "defender not dominant: `persona_fort` vs `shaper` {}, vs `search` {}; the cheap defender, `persona_rush` vs `persona_turtle`: {}",
                    pm(self.rate("fort_vs_shaper")),
                    pm(self.rate("fort_vs_search")),
                    pm(self.rate("rush_vs_turtle"))
                ),
            },
            Criterion {
                id: "C9",
                claim: "The tension sits on decisions",
                measure: "per-phase movement of win probability",
                value: "-".into(),
                target: "build + attack \u{2265} 50%; deal \u{2264} 15%",
                status: Status::Pending,
                note: "the tension curve is not built".into(),
            },
            Criterion {
                id: "C10",
                claim: "The feel checks hold",
                measure: "close finish, wire-to-wire, lead changes, comeback in `search` self-play, each against its bar",
                value: if feel_bad.is_empty() {
                    "all inside".into()
                } else {
                    format!("outside: {}", feel_bad.join(", "))
                },
                target: "none outside its bar by more than its CI",
                status: Status::of(feel_bad.is_empty()),
                note: String::new(),
            },
            Criterion {
                id: "C11",
                claim: "A round fits a phone",
                measure: "decisions per round; legal ops per shaping decision (`shaper` self-play)",
                value: format!("{:.1} and {:.0}", self.decisions_per_round(), sh.per_round[0]),
                target: "reported",
                status: Status::Reported,
                note: "judged in playtests".into(),
            },
        ]
    }

    fn checks(&self) -> Vec<Check> {
        let (sp, sh) = (&self.search_sp, &self.shaper_sp);
        let seat = |name, p: &Pooled| {
            let ci = 1.96 * (0.25 / p.n.max(1) as f64).sqrt();
            Check {
                name,
                value: format!("{:.1}% seat A", p.seat_a * 100.0),
                bar: format!("50% \u{b1}{:.1} (95% CI at n={})", ci * 100.0, p.n),
                ok: (p.seat_a - 0.5).abs() <= ci,
            }
        };
        let max = |a: &[f64; 7]| a.iter().cloned().fold(0.0, f64::max);
        let dead = dead_ops(&sp.count_share);
        let selfcheck = sp.probe.selfcheck_fail;
        vec![
            seat("Seat bias (`search` mirror)", sp),
            seat("Seat bias (`shaper` mirror)", sh),
            Check {
                name: "Match length (median rounds)",
                value: format!("{:.0}", sp.median_rounds),
                bar: "3-4".into(),
                ok: (3.0..=4.0).contains(&sp.median_rounds),
            },
            Check {
                name: "Games reaching round 5",
                value: pc(sp.reach_r5),
                bar: "< 10%".into(),
                ok: sp.reach_r5 < 0.10,
            },
            Check {
                name: "Median first core damage",
                value: format!("round {:.0}", sp.median_first_dmg),
                bar: "\u{2264} round 2".into(),
                ok: sp.median_first_dmg <= 2.0,
            },
            Check {
                name: "Max single-op share of TP",
                value: pc(max(&sp.tp_share)),
                bar: "\u{2264} 50%".into(),
                ok: max(&sp.tp_share) <= 0.5,
            },
            Check {
                name: "Max single-op share of op count",
                value: pc(max(&sp.count_share)),
                bar: "\u{2264} 50%".into(),
                ok: max(&sp.count_share) <= 0.5,
            },
            Check {
                name: "Dead ops (< 2% of ops)",
                value: if dead.is_empty() { "none".into() } else { dead.join(", ") },
                bar: "none".into(),
                ok: dead.is_empty(),
            },
            Check {
                name: "Counterfactual self-check failures",
                value: selfcheck.to_string(),
                bar: "0".into(),
                ok: selfcheck == 0,
            },
        ]
    }

    pub fn json(&self) -> Value {
        let pr = &self.search_sp.probe;
        let rates: serde_json::Map<String, Value> = self
            .rates
            .iter()
            .map(|&(key, a, b, r)| (key.to_string(), json!({"a": a, "b": b, "winrate": r4(r.p), "ci": r4(r.ci), "n": r.n})))
            .collect();
        let criteria: Vec<Value> = self
            .criteria()
            .iter()
            .map(|c| json!({"id": c.id, "claim": c.claim, "measure": c.measure, "value": c.value, "target": c.target, "status": c.status.name(), "note": c.note}))
            .collect();
        let checks: Vec<Value> = self
            .checks()
            .iter()
            .map(|c| json!({"name": c.name, "value": c.value, "bar": c.bar, "ok": c.ok}))
            .collect();
        let feels: Vec<Value> = self
            .feels()
            .iter()
            .map(|f| json!({"key": f.key, "name": f.name, "value": r4(f.value), "ci": r4(f.ci), "low": f.band.0, "high": f.band.1, "show": f.show(), "bar": f.bar(), "verdict": f.verdict(), "fails": f.fails()}))
            .collect();
        let commanders: Vec<Value> = (0..4)
            .map(|i| {
                let (p, ci) = rate_ci(self.search_sp.cmd_pts[i], self.search_sp.cmd_n[i]);
                json!({"name": ALL_COMMANDERS[i].name(), "points": r4(p), "ci": r4(ci), "n": self.search_sp.cmd_n[i]})
            })
            .collect();
        let [legal, ops, built, bought] = self.shaper_sp.per_round;
        json!({
            "version": VERSION,
            "games": self.games,
            "criteria": criteria,
            "checks": checks,
            "head_to_head": rates,
            "feel": feels,
            "selfplay": {"search": selfplay_json(&self.search_sp), "shaper": selfplay_json(&self.shaper_sp)},
            "crossing": {
                "rounds": pr.rounds,
                "guess_share": r4(self.guess_share()),
                "swing": r4(Self::mean(pr.swing_sum as f64, pr.rounds)),
                "guess_gap": r4(Self::mean(pr.gap_sum, pr.rounds)),
                "big_guess_share": r4(Self::mean(pr.big_guess_rounds as f64, pr.rounds)),
                "interaction_share": r4(if pr.total_ss > 0.0 { pr.inter_ss / pr.total_ss } else { f64::NAN }),
                "op_value": r4(Self::mean(pr.op_value_sum as f64, pr.op_count)),
                "ops_valued": pr.op_count,
                "selfcheck_fail": pr.selfcheck_fail,
            },
            "deals": {
                "share": r4(self.deals.share),
                "between_var": self.deals.between,
                "within_var": self.deals.within,
                "lopsided": r4(self.deals.lopsided),
                "seat_a": r4(self.deals.seat_a),
                "deals": self.deals.deals,
                "replays": self.deals.replays,
            },
            "round": {"decisions": r4(self.decisions_per_round()), "legal_ops": r4(legal), "ops": r4(ops), "buildings": r4(built), "units": r4(bought)},
            "commanders": {"style": style_label(), "rows": commanders},
            "secs": (self.secs * 10.0).round() / 10.0,
        })
    }

    pub fn markdown(&self) -> String {
        let (sp, sh, pr) = (&self.search_sp, &self.shaper_sp, &self.search_sp.probe);
        let criteria = self.criteria();
        let checks = self.checks();
        let mut md = String::new();
        let _ = writeln!(md, "# Mirror Terrain scorecard (rules v{VERSION})\n");
        let _ = writeln!(
            md,
            "Criteria C1-C11 of `docs/SPEC.md` and the match checks of `docs/METRICS.md`. {} games per matchup, seat-swapped. \
             A 95% CI follows each rate. *pending* = the simulator cannot measure it yet. *reported* = measured, no bar.\n",
            self.games
        );
        let scored: Vec<&Criterion> = criteria.iter().filter(|c| matches!(c.status, Status::Pass | Status::Fail)).collect();
        let _ = writeln!(
            md,
            "**{} of {} scored criteria pass; {} of {} match checks pass.**\n",
            scored.iter().filter(|c| c.status == Status::Pass).count(),
            scored.len(),
            checks.iter().filter(|c| c.ok).count(),
            checks.len()
        );
        let _ = writeln!(
            md,
            "## Criteria\n\n| # | Claim | Measured as | Result | Target | Verdict | Note |\n| --- | --- | --- | --- | --- | --- | --- |"
        );
        for c in &criteria {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} | {} |",
                c.id,
                c.claim,
                c.measure,
                c.value,
                c.target,
                c.status.md(),
                c.note
            );
        }
        let _ = writeln!(
            md,
            "\n## Match checks (`search` self-play unless named)\n\n| Check | Result | Bar | Verdict |\n| --- | --- | --- | --- |"
        );
        for c in &checks {
            let _ = writeln!(md, "| {} | {} | {} | {} |", c.name, c.value, c.bar, if c.ok { "PASS" } else { "**FAIL**" });
        }

        let _ = writeln!(md, "\n## Head-to-head\n\n| Matchup | Win rate of the first bot | Games |\n| --- | --- | --- |");
        for &(_, a, b, r) in &self.rates {
            let _ = writeln!(md, "| `{a}` vs `{b}` | {} | {} |", pm(r), r.n);
        }

        let _ = writeln!(
            md,
            "\n## Where the C1 edge comes from\n\nAll five bots share one build search; they differ in shaping and in building placement.\n"
        );
        let _ = writeln!(md, "| Matchup | Shaping | Placement | Win rate |\n| --- | --- | --- | --- |");
        for (name, shaping, placement, key) in [
            (
                "`shaper` vs `no_shape` (C1)",
                "searched vs none",
                "searched vs default",
                "c1_shaper_vs_no_shape",
            ),
            ("`shape_only` vs `no_shape`", "searched vs none", "default both", "c1_shape_only_vs_no_shape"),
            ("`place_only` vs `no_shape`", "none both", "searched vs default", "c1_place_only_vs_no_shape"),
            (
                "`shaper` vs `rand_shape` (C2)",
                "searched vs random",
                "searched both",
                "c2_shaper_vs_rand_shape",
            ),
            ("`shaper` vs `search`", "searched vs sampled", "searched vs default", "shaper_vs_search"),
        ] {
            let _ = writeln!(md, "| {name} | {shaping} | {placement} | {} |", pm(self.rate(key)));
        }

        let _ = writeln!(md, "\n## Shaping\n\n| | `shaper` self-play | `search` self-play |\n| --- | --- | --- |");
        let dead = |p: &Pooled| {
            let d = dead_ops(&p.count_share);
            if d.is_empty() {
                "none".to_string()
            } else {
                d.join(", ")
            }
        };
        let _ = writeln!(
            md,
            "| Shaping decisions where no op helped | {} of {:.0} | - |",
            pc(sh.nohelp_share),
            sh.nohelp_eligible
        );
        let _ = writeln!(md, "| TP used | {} | {} |", pc(sh.tp_use), pc(sp.tp_use));
        let _ = writeln!(md, "| Dead ops (< 2% of ops) | {} | {} |", dead(sh), dead(sp));
        let _ = writeln!(
            md,
            "| Blocking ops reverted for path safety | {} | {} |",
            pc(sh.reverted_rate),
            pc(sp.reverted_rate)
        );
        let _ = writeln!(
            md,
            "\n| Op | Share of ops, `shaper` | Share of ops, `search` | Share of TP, `search` |\n| --- | --- | --- | --- |"
        );
        for (i, o) in OPS.iter().enumerate() {
            let _ = writeln!(md, "| {o} | {} | {} | {} |", pc(sh.count_share[i]), pc(sp.count_share[i]), pc(sp.tp_share[i]));
        }
        let _ = writeln!(
            md,
            "\nOne applied op moves the round by {:.2} core HP on average ({} ops, `search` self-play, the op removed and everything else replayed).",
            Self::mean(pr.op_value_sum as f64, pr.op_count),
            pr.op_count
        );

        let _ = writeln!(
            md,
            "\n## The crossing (`search` self-play, {} rounds)\n\nEach round, all 81 pairs of crossings are replayed on clones with everything else as played.\n",
            pr.rounds
        );
        let _ = writeln!(md, "| | |\n| --- | --- |");
        let _ = writeln!(md, "| Rounds with no pure equilibrium (a guess) | {} |", pc(self.guess_share()));
        let _ = writeln!(
            md,
            "| Mean guess gap (minimax - maximin, eval units) | {:.2} |",
            Self::mean(pr.gap_sum, pr.rounds)
        );
        let _ = writeln!(
            md,
            "| Big guesses (gap \u{2265} 4 core HP) | {} |",
            pc(Self::mean(pr.big_guess_rounds as f64, pr.rounds))
        );
        let _ = writeln!(
            md,
            "| Mean swing over the 81 cells (core HP) | {:.1} |",
            Self::mean(pr.swing_sum as f64, pr.rounds)
        );
        let _ = writeln!(
            md,
            "| Interaction share of the core-HP matrix | {} |",
            pc(if pr.total_ss > 0.0 { pr.inter_ss / pr.total_ss } else { f64::NAN })
        );
        let _ = writeln!(
            md,
            "| Crossing repeats last round's column | {} (`search`), {} (`shaper`) |",
            pc(sp.cross_repeat),
            pc(sh.cross_repeat)
        );

        let _ = writeln!(
            md,
            "\n## C10: feel (`search` self-play)\n\n| Metric | Result | Bar | Verdict |\n| --- | --- | --- | --- |"
        );
        for f in self.feels() {
            let _ = writeln!(md, "| {} | {} | {} | {} |", f.name, f.show(), f.bar(), f.verdict());
        }
        let _ = writeln!(
            md,
            "\nReported: games decided on HP {} (knockout {}); live going into the last round {}.",
            pc(sp.hp_decided),
            pc(1.0 - sp.hp_decided),
            pc(sp.live_last_round)
        );

        let [legal, ops, built, bought] = sh.per_round;
        let _ = writeln!(
            md,
            "\n## C11: one round for one player (`shaper` self-play)\n\n| Decision | Per round | Options |\n| --- | --- | --- |"
        );
        let _ = writeln!(md, "| Draft | 1 | a ranked order of 3 cards |");
        let _ = writeln!(md, "| Shaping ops | {ops:.2} | {legal:.0} legal (tile, op) choices |");
        let _ = writeln!(md, "| Buildings placed | {built:.2} | own build zone |");
        let _ = writeln!(md, "| Units bought | {bought:.2} | unlocked kinds |");
        let _ = writeln!(md, "| Crossing | 1 | 9 columns |");
        let _ = writeln!(md, "| **Total** | **{:.1}** | |", self.decisions_per_round());

        let _ = writeln!(
            md,
            "\n## Commanders (`search` self-play, bot style `{}`)\n\n| Commander | Points | Games |\n| --- | --- | --- |",
            style_label()
        );
        for i in 0..4 {
            let (p, ci) = rate_ci(sp.cmd_pts[i], sp.cmd_n[i]);
            let _ = writeln!(
                md,
                "| {} | {:.1}% \u{b1}{:.1} | {} |",
                ALL_COMMANDERS[i].name(),
                p * 100.0,
                ci * 100.0,
                sp.cmd_n[i]
            );
        }
        let _ = writeln!(
            md,
            "\nA tripwire only: both seats of a seed share its commander pair, so the CI is optimistic. `mt_sim commanders` is the precise tool."
        );

        let _ = writeln!(md, "\n## Kill criteria (`docs/SPEC.md`; decide at 1,600 games)\n");
        let flag = |bad: bool| if bad { "**would trip**" } else { "clear" };
        let (c1, rush_fort) = (self.rate("c1_shaper_vs_no_shape"), self.rate("c8_rush_vs_fort"));
        let _ = writeln!(md, "- C1 below 55%: {} ({}).", flag(c1.p < 0.55), pm(c1));
        let _ = writeln!(
            md,
            "- Crossing guess share above 25%: {} ({}).",
            flag(self.guess_share() > 0.25),
            pc(self.guess_share())
        );
        let _ = writeln!(
            md,
            "- Rush vs the defender outside 35-65%: {} ({}).",
            flag(!(0.35..=0.65).contains(&rush_fort.p)),
            pm(rush_fort)
        );
        let _ = writeln!(md, "\nTotal runtime: {:.0}s.", self.secs);
        md
    }
}

/// The scorecard written to `outdir` as `report.md` and `report.json`, plus five recorded games for the viewer.
pub fn suite(outdir: &str, games: usize) {
    std::fs::create_dir_all(format!("{outdir}/replays")).ok();
    let card = measure(games);
    for (name, a, b, seed) in REPLAYS {
        let mut ba = make_bot(a, seed * 3 + 1);
        let mut bb = make_bot(b, seed * 3 + 2);
        if let (_, Some(json)) = play(seed, ba.as_mut(), bb.as_mut(), true, Instr::Off) {
            std::fs::write(format!("{outdir}/replays/{name}.json"), json).ok();
        }
    }
    let md = card.markdown();
    std::fs::write(format!("{outdir}/report.md"), &md).ok();
    std::fs::write(format!("{outdir}/report.json"), format!("{}\n", card.json())).ok();
    println!("{md}");
}

/// The games `suite` records: (file name, seat A, seat B, seed). `viewer/build_viewer.py` lists the same names.
const REPLAYS: [(&str, &str, &str, u64); 5] = [
    ("search_vs_search", "search", "search", 42),
    ("shaper_vs_search", "shaper", "search", 43),
    ("air_vs_ground", "persona_air", "persona_ground", 44),
    ("rush_vs_fort", "persona_rush", "persona_fort", 45),
    ("search_vs_greedy", "search", "greedy", 46),
];
