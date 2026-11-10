//! Counterfactual probes. They run on `sim_clone()`s, never call a bot and never draw from an RNG, so they cannot
//! change a game; `mt_sim verify` plays the same seed with probes off and on and fails unless the hash matches.

use mt_bots::eval;
use mt_core::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Instr {
    Off,
    /// Counters only (legal ops, decisions, "no op helps").
    Cheap,
    /// Counters plus the per-round crossing matrix and op values.
    Full,
}

#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub rounds: i32,
    /// Rounds whose crossing game has no pure equilibrium in `eval` units.
    pub guess_rounds: i32,
    /// Sum over rounds of max - min core-HP difference over the 81 crossing cells.
    pub swing_sum: i32,
    /// Sum over rounds of minimax - maximin of the crossing matrix, in `eval` units (the size of the guess).
    pub gap_sum: f64,
    /// Rounds whose core-HP matrix has minimax - maximin >= `BIG_GUESS_HP`.
    pub big_guess_rounds: i32,
    /// Two-way decomposition of the core-HP-difference matrix, summed over rounds: the row x column interaction sum of
    /// squares, and the total sum of squares about the grand mean.
    pub inter_ss: f64,
    pub total_ss: f64,
    /// Sum and count of op values: the absolute change in the round's core-HP difference when one applied op is removed.
    pub op_value_sum: i32,
    pub op_count: i32,
    pub selfcheck_fail: i32,
}

impl Probe {
    pub fn add(&mut self, o: &Probe) {
        self.rounds += o.rounds;
        self.guess_rounds += o.guess_rounds;
        self.swing_sum += o.swing_sum;
        self.gap_sum += o.gap_sum;
        self.big_guess_rounds += o.big_guess_rounds;
        self.inter_ss += o.inter_ss;
        self.total_ss += o.total_ss;
        self.op_value_sum += o.op_value_sum;
        self.op_count += o.op_count;
        self.selfcheck_fail += o.selfcheck_fail;
    }
}

/// A guess is "big" when guessing wrong costs this many core HP (minimax - maximin of the HP matrix).
pub const BIG_GUESS_HP: f64 = 4.0;

/// (interaction sum of squares, total sum of squares about the grand mean) of a matrix. The two-way decomposition
/// total = rows + columns + interaction; the interaction is what no row-only or column-only reading can explain, i.e.
/// "do the armies meet". `(0, 0)` for a matrix with fewer than two rows or columns.
pub fn interaction_ss(m: &[Vec<f64>]) -> (f64, f64) {
    let (rows, cols) = (m.len(), m.first().map_or(0, |r| r.len()));
    if rows < 2 || cols < 2 {
        return (0.0, 0.0);
    }
    let n = (rows * cols) as f64;
    let grand = m.iter().flatten().sum::<f64>() / n;
    let row_mean: Vec<f64> = m.iter().map(|r| r.iter().sum::<f64>() / cols as f64).collect();
    let col_mean: Vec<f64> = (0..cols).map(|j| m.iter().map(|r| r[j]).sum::<f64>() / rows as f64).collect();
    let (mut inter, mut total) = (0.0, 0.0);
    for i in 0..rows {
        for j in 0..cols {
            let r = m[i][j] - row_mean[i] - col_mean[j] + grand;
            inter += r * r;
            total += (m[i][j] - grand).powi(2);
        }
    }
    (inter, total)
}

fn hp_diff(g: &Game) -> i32 {
    g.players[0].core_hp - g.players[1].core_hp
}

/// For a bot that shapes by search: `None` when the decision had nothing to decide (no legal op fits the budget),
/// otherwise whether the bot chose to do nothing.
pub fn no_op_helps(g: &Game, side: usize, plan: &[(usize, Op)]) -> Option<bool> {
    let fits = g.legal_ops(side).iter().any(|&(_, op)| op.cost() <= g.tp(side));
    fits.then_some(plan.is_empty())
}

/// (maximin, minimax) of a payoff matrix for the row player. A pure equilibrium exists iff they are equal.
fn pure_bounds(m: &[Vec<f64>]) -> (f64, f64) {
    let maximin = m
        .iter()
        .map(|r| r.iter().cloned().fold(f64::INFINITY, f64::min))
        .fold(f64::NEG_INFINITY, f64::max);
    let cols = m.first().map_or(0, |r| r.len());
    let minimax = (0..cols)
        .map(|j| m.iter().map(|r| r[j]).fold(f64::NEG_INFINITY, f64::max))
        .fold(f64::INFINITY, f64::min);
    (maximin, minimax)
}

pub fn needs_a_guess(m: &[Vec<f64>], eps: f64) -> bool {
    let (maximin, minimax) = pure_bounds(m);
    maximin < minimax - eps
}

/// minimax - maximin: how much the row player can lose or gain by guessing wrong about the column player. Zero when
/// a pure equilibrium exists.
pub fn guess_gap(m: &[Vec<f64>]) -> f64 {
    let (maximin, minimax) = pure_bounds(m);
    (minimax - maximin).max(0.0)
}

/// One round's probes. `pre` is the game after the draft; `clean` the sanitised plans and `builds` the builds that were
/// played; `built` the game after the build, before the attack; `cross` the crossings played; `after` the real game
/// after the clash.
pub fn probe_round(pre: &Game, clean: &[Vec<(usize, Op)>; 2], builds: &[BuildCmd; 2], built: &Game, cross: [u8; 2], after: &Game, out: &mut Probe) {
    out.rounds += 1;
    // The crossing game: every column forced for both sides, everything else as played, true armies included.
    let dim = CROSSINGS.len();
    let mut m = vec![vec![0.0; dim]; dim];
    let mut hp = vec![vec![0.0; dim]; dim];
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for (ia, &ca) in CROSSINGS.iter().enumerate() {
        for (ib, &cb) in CROSSINGS.iter().enumerate() {
            let mut c = built.sim_clone();
            c.apply_attack([ca, cb]);
            c.clash_and_resolve();
            m[ia][ib] = eval(&c, 0);
            let d = hp_diff(&c);
            hp[ia][ib] = d as f64;
            lo = lo.min(d);
            hi = hi.max(d);
        }
    }
    out.swing_sum += hi - lo;
    out.gap_sum += guess_gap(&m);
    out.guess_rounds += i32::from(needs_a_guess(&m, 1e-9));
    out.big_guess_rounds += i32::from(guess_gap(&hp) >= BIG_GUESS_HP);
    let (inter, total) = interaction_ss(&hp);
    out.inter_ss += inter;
    out.total_ss += total;

    // Op values: replay the round without one op. Replaying it unchanged must reproduce the real round.
    let d_with = hp_diff(after);
    let replay = |plans: [Vec<(usize, Op)>; 2]| -> i32 {
        let mut c = pre.sim_clone();
        c.apply_terraform(plans);
        c.apply_build(builds.clone());
        c.apply_attack(cross);
        c.clash_and_resolve();
        hp_diff(&c)
    };
    if replay(clean.clone()) != d_with {
        out.selfcheck_fail += 1;
    }
    for rec in after.last_ops.iter().filter(|r| r.outcome == OpOutcome::Applied) {
        let mut plans = clean.clone();
        plans[rec.side].retain(|&(t, _)| t != rec.tile);
        out.op_value_sum += (d_with - replay(plans)).abs();
        out.op_count += 1;
    }
}

/// One-way random-effects split of outcome variance: (between deals, within a deal, between / total).
pub fn variance_share(deals: &[Vec<f64>]) -> (f64, f64, f64) {
    let d = deals.len();
    let r = deals.first().map_or(0, |v| v.len());
    if d < 2 || r < 2 || deals.iter().any(|v| v.len() != r) {
        return (0.0, 0.0, 0.0);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let var = |v: &[f64]| {
        let m = mean(v);
        v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64
    };
    let means: Vec<f64> = deals.iter().map(|v| mean(v)).collect();
    let within = deals.iter().map(|v| var(v)).sum::<f64>() / d as f64;
    let between = (var(&means) - within / r as f64).max(0.0);
    let total = between + within;
    (between, within, if total > 0.0 { between / total } else { 0.0 })
}

pub fn lopsided_share(deals: &[Vec<f64>]) -> f64 {
    let n = deals.len().max(1) as f64;
    let c = deals
        .iter()
        .filter(|v| {
            let m = v.iter().sum::<f64>() / v.len().max(1) as f64;
            m >= 0.9 - 1e-12 || m <= 0.1 + 1e-12
        })
        .count();
    c as f64 / n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variance_share_all_fixed_by_the_deal() {
        let deals: Vec<Vec<f64>> = (0..20).map(|d| vec![(d % 2) as f64; 10]).collect();
        let (between, within, share) = variance_share(&deals);
        assert!(within.abs() < 1e-12 && between > 0.2);
        assert!((share - 1.0).abs() < 1e-12);
    }

    #[test]
    fn variance_share_pure_coin_flips_is_about_zero() {
        let mut x: u64 = 12345;
        let mut flip = || {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            f64::from((x >> 33) & 1 == 1)
        };
        let deals: Vec<Vec<f64>> = (0..400).map(|_| (0..10).map(|_| flip()).collect()).collect();
        let (_, within, share) = variance_share(&deals);
        assert!((within - 0.25).abs() < 0.02, "within {within}");
        assert!(share < 0.03, "share {share}");
    }

    #[test]
    fn variance_share_hand_checked() {
        let (b, w, s) = variance_share(&[vec![0.0, 0.5], vec![0.5, 1.0]]);
        assert!((b - 0.0625).abs() < 1e-12 && (w - 0.125).abs() < 1e-12 && (s - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(variance_share(&[vec![0.0, 1.0], vec![1.0, 0.0]]).2, 0.0);
    }

    #[test]
    fn lopsided_counts_both_seats() {
        let deals = vec![vec![1.0; 10], vec![0.0; 10], vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]];
        assert!((lopsided_share(&deals) - 2.0 / 3.0).abs() < 1e-12);
        let mut nine = vec![1.0; 9];
        nine.push(0.0);
        assert_eq!(lopsided_share(&[nine]), 1.0);
    }

    #[test]
    fn interaction_ss_matches_hand_decomposition() {
        // purely additive (row effect + column effect): no interaction
        let additive = vec![vec![1.0, 2.0, 4.0], vec![3.0, 4.0, 6.0]];
        let (i, t) = interaction_ss(&additive);
        assert!(i.abs() < 1e-12 && t > 0.0);
        // a 2x2 coin-matching game is all interaction
        let (i, t) = interaction_ss(&[vec![1.0, -1.0], vec![-1.0, 1.0]]);
        assert!((i - 4.0).abs() < 1e-12 && (t - 4.0).abs() < 1e-12);
        // a constant matrix has nothing to explain; a single row or column is not a game
        assert_eq!(interaction_ss(&[vec![2.0, 2.0], vec![2.0, 2.0]]), (0.0, 0.0));
        assert_eq!(interaction_ss(&[vec![1.0, 5.0, 3.0]]), (0.0, 0.0));
        // [[0,4],[2,2]]: grand 2, row means 2 2, col means 1 3: residuals -1 +1 / +1 -1 => 4 of total 8
        let (i, t) = interaction_ss(&[vec![0.0, 4.0], vec![2.0, 2.0]]);
        assert!((i - 4.0).abs() < 1e-12 && (t - 8.0).abs() < 1e-12);
    }

    #[test]
    fn saddle_matrix_has_a_pure_equilibrium() {
        assert!(!needs_a_guess(&[vec![1.0, 5.0], vec![3.0, 4.0]], 1e-9));
        assert!(!needs_a_guess(&[vec![1.0, 1.0 + 1e-12], vec![1.0 + 1e-12, 1.0]], 1e-9));
    }

    #[test]
    fn rock_paper_scissors_needs_a_guess() {
        let rps = vec![vec![0.0, -1.0, 1.0], vec![1.0, 0.0, -1.0], vec![-1.0, 1.0, 0.0]];
        assert!(needs_a_guess(&rps, 1e-9));
        assert!(needs_a_guess(&[vec![1.0, 4.0], vec![4.0, 1.0]], 1e-9));
    }
}
