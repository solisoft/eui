//! Where a beam lies, as LilyPond's beam quanting puts it (score #410,
//! `Beam_scoring_problem`): the ideal place from the stems' ideal lengths
//! (least squares), its slope damped, shifted until no stem is too short;
//! then every place nearby whose ends sit on a staff line, straddle it,
//! hang from it or lie half way between two lines is scored — stems too
//! short or longer than ideal, a slope against the notes' or steeper than
//! they go, a staff line caught in a thin wedge of white beside a beam —
//! and the cheapest one taken. A concave beam (an inner note nearer the
//! beam than both ends) is flat.
//!
//! Lengths in staff spaces, y up (LilyPond's sense), from whatever origin
//! the caller gives; a staff's lines are at whole spaces from `origin`.

/// LilyPond's `beamed-lengths`: a beamed stem's ideal length, head to the
/// outer edge of its outermost beam, by the beams it carries.
const BEAMED_LENGTHS: [f64; 3] = [3.26, 3.5, 3.6];
/// `beamed-minimum-free-lengths`: the least free stem between the head and
/// its innermost beam, ideally.
const MIN_FREE: [f64; 3] = [1.83, 1.5, 1.25];
/// `beamed-extreme-minimum-free-lengths`: … and at the least.
const EXTREME_FREE: [f64; 2] = [2.0, 1.25];

// `Beam_quant_parameters`' defaults.
const STEM_LENGTH_LIMIT_PENALTY: f64 = 5000.0;
const STEM_LENGTH_DEMERIT_FACTOR: f64 = 5.0;
const SECONDARY_BEAM_DEMERIT: f64 = 15.0;
const DAMPING_DIRECTION_PENALTY: f64 = 800.0;
const HINT_DIRECTION_PENALTY: f64 = 20.0;
const MUSICAL_DIRECTION_FACTOR: f64 = 400.0;
const IDEAL_SLOPE_FACTOR: f64 = 10.0;
const ROUND_TO_ZERO_SLOPE: f64 = 0.02;
const HORIZONTAL_INTER_QUANT_PENALTY: f64 = 500.0;
const REGION_SIZE: i64 = 2;
const EPS: f64 = 1e-3;

/// A stem under the beam.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QStem {
    /// Its x.
    pub x: f64,
    /// Its head nearest the beam's middle, y up.
    pub head: f64,
    /// 1: the beam is over it; -1: under it.
    pub dir: f64,
    /// How far its beams reach from the outermost one's middle towards its
    /// head, to their inner edge.
    pub stack: f64,
}

/// What shapes a beam besides its stems.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QBeam<'a> {
    /// The most beams it has.
    pub count: i64,
    /// Beams at its first stem and at its last.
    pub edge_counts: (i64, i64),
    /// A beam's thickness.
    pub thick: f64,
    /// From one beam to the next.
    pub step: f64,
    /// A staff line's thickness.
    pub line: f64,
    /// Lengths scaled so (a grace note's are shorter).
    pub scale: f64,
    /// `Beam.damping`: 1 by default, infinite for flat.
    pub damping: f64,
    /// A staff line's y: the grid of quants is measured from it.
    pub origin: f64,
    /// The staff lines a beam end may meet, y up.
    pub lines: &'a [f64],
    /// The staff's middle line and its outer lines (bottom, top), which a
    /// beam in one staff reaches to; none for a beam across two staves.
    pub staff: Option<(f64, f64, f64)>,
}

/// `shrink_extra_weight`: a shortfall costs `fac` times more.
fn shrink_extra_weight(x: f64, fac: f64) -> f64 {
    x.abs() * if x < 0.0 { fac } else { 1.0 }
}

fn sign(x: f64) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// `x - floor(x)`.
fn modf(x: f64) -> f64 {
    x - x.floor()
}

/// The middle of the outermost beam at the first stem and at the last, y
/// up, as LilyPond quants it.
pub(crate) fn place(stems: &[QStem], b: &QBeam<'_>) -> (f64, f64) {
    let n = stems.len();
    let x0 = stems[0].x;
    let dx = stems[n - 1].x - x0;
    let k = (b.count.max(1) - 1) as usize;
    let half = b.thick / 2.0;
    // Each stem's ideal and least ends, at the outermost beam's middle.
    let infos: Vec<(f64, f64)> = stems
        .iter()
        .map(|s| {
            let ideal_len = ((BEAMED_LENGTHS[k.min(2)] * b.scale) - half).max(MIN_FREE[k.min(2)] * b.scale + s.stack);
            let least_len = EXTREME_FREE[k.min(1)] * b.scale + s.stack;
            let start = s.head * s.dir;
            let mut ideal = start + ideal_len;
            if let Some((middle, bottom, top)) = b.staff {
                // To the middle line at least, the innermost beam not past the
                // outer line.
                let (mid, outer) = if s.dir > 0.0 { (middle, bottom) } else { (-middle, -top) };
                ideal = ideal.max(mid).max(outer + half + (b.count - 1) as f64 * b.step);
            }
            (ideal * s.dir, (start + least_len) * s.dir)
        })
        .collect();
    let at = |y: (f64, f64), x: f64| if dx.abs() < 1e-9 { (y.0 + y.1) / 2.0 } else { y.0 + (y.1 - y.0) * (x - x0) / dx };
    let min_dy = |dy: f64| {
        if dy == 0.0 {
            return 0.0;
        }
        let sit = (b.thick - b.line) / 2.0;
        dy.signum() * dy.abs().max(sit.min(0.5).min(1.0 - sit))
    };
    // The least-squares line through the ideal ends.
    let (mut pos, musical) = if (infos[0].0 - infos[n - 1].0).abs() < 1e-9 {
        let mut p = (infos[0].0, infos[n - 1].0);
        // Two notes whose stems both reach the middle line: slightly sloped.
        if let Some((middle, _, _)) = b.staff {
            let chord = stems[n - 1].head - stems[0].head;
            if n == 2 && (p.0 - middle).abs() < 1e-9 && chord.abs() > 1e-9 {
                let d = chord.signum();
                p = (middle - d * half, middle + d * half);
            }
        }
        (p, p.1 - p.0)
    } else if stems.iter().any(|s| s.dir != stems[0].dir) {
        // A knee: the stems of each side on a line of their own, the two
        // parallel — their slope is the music's — and the beam half way.
        let (slope, y) = knee_fit(stems, &infos, x0);
        let dy = min_dy(slope * dx);
        ((y, y + dy), dy)
    } else {
        let m = n as f64;
        let sx: f64 = stems.iter().map(|s| s.x - x0).sum();
        let sy: f64 = infos.iter().map(|i| i.0).sum();
        let sxx: f64 = stems.iter().map(|s| (s.x - x0) * (s.x - x0)).sum();
        let sxy: f64 = stems.iter().zip(&infos).map(|(s, i)| (s.x - x0) * i.0).sum();
        let den = m * sxx - sx * sx;
        let slope = if den.abs() < 1e-9 { 0.0 } else { (m * sxy - sx * sy) / den };
        let y = (sy - slope * sx) / m;
        let dy = min_dy(slope * dx);
        ((y, y + dy), dy)
    };
    // Concave (an inner note nearer the beam than both ends): flat.
    let concave = n > 2 && {
        let d = stems[0].dir;
        let ends = (stems[0].head * d).max(stems[n - 1].head * d);
        stems.iter().all(|s| s.dir == d) && stems[1..n - 1].iter().any(|s| s.head * d > ends + 1e-9)
    };
    // Damped: LilyPond's `0.6 tanh(slope) / damping`, flat when concave.
    if concave || b.damping.is_infinite() {
        let c = (pos.0 + pos.1) / 2.0;
        pos = (c, c);
    } else if b.damping > 0.0 && dx > 1e-9 {
        let dy = pos.1 - pos.0;
        let slope = 0.6 * (dy / dx).tanh() / b.damping;
        let damped = min_dy(slope * dx);
        pos = (pos.0 + (dy - damped) / 2.0, pos.1 - (dy - damped) / 2.0);
    }
    // Shifted until no stem is shorter than its least (`shift_region_to_valid`).
    let mut lift: f64 = 0.0;
    let mut drop: f64 = 0.0;
    for (s, i) in stems.iter().zip(&infos) {
        let y = at(pos, s.x);
        if s.dir > 0.0 {
            lift = lift.max(i.1 - y);
        } else {
            drop = drop.max(y - i.1);
        }
    }
    if lift > 0.0 && drop <= 0.0 {
        pos = (pos.0 + lift, pos.1 + lift);
    } else if drop > 0.0 && lift <= 0.0 {
        pos = (pos.0 - drop, pos.1 - drop);
    }
    let damped_dy = pos.1 - pos.0;
    let xstaff = b.staff.is_none();
    // The quants: on a line, sitting on it, half way, hanging from it.
    let sit = (b.thick - b.line) / 2.0;
    let quants = [0.0, sit, 0.5, 1.0 - sit];
    let grid = |y: f64| -> Vec<f64> {
        let base = (y - b.origin).floor();
        let mut out = Vec::with_capacity(quants.len() * (2 * REGION_SIZE as usize + 1));
        for i in -REGION_SIZE..=REGION_SIZE {
            for q in quants {
                out.push(b.origin + base + i as f64 + q);
            }
        }
        out
    };
    let (left, right) = (grid(pos.0), grid(pos.1));
    let extra = SECONDARY_BEAM_DEMERIT / b.edge_counts.0.max(b.edge_counts.1).max(1) as f64;
    // The one nearest the unquanted place first: a good score found early
    // cuts the others short.
    let nearest = |list: &[f64], y: f64| list.iter().copied().min_by(|a, c| (a - y).abs().total_cmp(&(c - y).abs())).unwrap_or(y);
    let first = (nearest(&left, pos.0), nearest(&right, pos.1));
    let configs = std::iter::once(first).chain(left.iter().flat_map(|&yl| right.iter().map(move |&yr| (yl, yr))));
    // Each stem's share of its side's average, and where it is along the beam.
    let ups = stems.iter().filter(|s| s.dir > 0.0).count().max(1) as f64;
    let downs = stems.iter().filter(|s| s.dir <= 0.0).count().max(1) as f64;
    let along: Vec<(f64, f64)> = stems.iter().map(|s| (if dx.abs() < 1e-9 { 0.5 } else { (s.x - x0) / dx }, if s.dir > 0.0 { 1.0 / ups } else { 1.0 / downs })).collect();
    let slope_k = IDEAL_SLOPE_FACTOR * if xstaff { 10.0 } else { 1.0 };
    let mut best = (f64::INFINITY, pos);
    for (yl, yr) in configs {
        // The slope: the notes' way, no steeper than they go, near the damped one.
        let dy = yr - yl;
        let mut score = 0.0;
        if sign(damped_dy) != sign(dy) {
            score += if dy == 0.0 && musical.abs() <= ROUND_TO_ZERO_SLOPE { HINT_DIRECTION_PENALTY } else { DAMPING_DIRECTION_PENALTY };
        }
        score += MUSICAL_DIRECTION_FACTOR * (dy.abs() - musical.abs()).max(0.0);
        score += shrink_extra_weight(damped_dy.abs() - dy.abs(), 1.5) * slope_k;
        if score >= best.0 {
            continue;
        }
        // The stems: none too short, each as near its ideal length as may
        // be, averaged over each side.
        let mut cut = false;
        for ((s, i), &(t, share)) in stems.iter().zip(&infos).zip(&along) {
            let y = yl + dy * t;
            let mut dem = STEM_LENGTH_LIMIT_PENALTY * (s.dir * (i.1 - y)).max(0.0);
            let off = shrink_extra_weight(s.dir * (y - i.0), 1.5);
            if off > 0.0 {
                dem += STEM_LENGTH_DEMERIT_FACTOR * off.powf(1.1);
            }
            score += dem * share;
            if score >= best.0 {
                cut = true;
                break;
            }
        }
        if cut {
            continue;
        }
        {
            // No staff line in a thin wedge of white inside a beam's edge.
            for (y, count, dir) in [(yl, b.edge_counts.0, stems[0].dir), (yr, b.edge_counts.1, stems[n - 1].dir)] {
                for j in 1..=count.max(1) {
                    let g1 = y - dir * ((j - 1) as f64 * b.step + half - b.line / 2.2);
                    let g2 = y - dir * (j as f64 * b.step - half + b.line / 2.2);
                    let (lo, hi) = (g1.min(g2), g1.max(g2));
                    for &l in b.lines {
                        if l >= lo && l <= hi {
                            let dist = (hi - l).abs().min((lo - l).abs());
                            score += extra * (0.39 + 0.61 * dist / (hi - lo).max(1e-9) * 2.0);
                        }
                    }
                }
            }
            // A second or third beam sitting where the first is told to.
            if b.edge_counts.0.max(b.edge_counts.1) >= 2 {
                for (y, count, dir) in [(yl, b.edge_counts.0, stems[0].dir), (yr, b.edge_counts.1, stems[n - 1].dir)] {
                    let inside = |shift: f64| b.staff.is_some_and(|(middle, bottom, top)| (y - dir * shift - middle).abs() < (top - bottom) / 2.0 + 0.5);
                    let rel = modf(y - b.origin);
                    if count >= 2 && inside(b.step) {
                        if dir > 0.0 && dy <= EPS && (rel - sit).abs() < EPS {
                            score += extra;
                        }
                        if dir < 0.0 && dy >= EPS && (rel - (1.0 - sit)).abs() < EPS {
                            score += extra;
                        }
                    }
                    if count >= 3 && inside(2.0 * b.step) && (dir > 0.0 && dy <= EPS || dir < 0.0 && dy >= EPS) && rel.min(1.0 - rel) < EPS {
                        score += extra;
                    }
                }
            }
            // A flat beam half way between two lines, inside the staff.
            if let Some((middle, bottom, top)) = b.staff {
                if dy == 0.0 && (yl - middle).abs() < (top - bottom) / 2.0 && (modf(yl - b.origin) - 0.5).abs() < 0.01 {
                    score += HORIZONTAL_INTER_QUANT_PENALTY;
                }
            }
            // Equal scores: the one nearer the unquanted place.
            score += 1e-6 * ((yl - pos.0).abs() + (yr - pos.1).abs());
            if score < best.0 {
                best = (score, (yl, yr));
            }
        }
    }
    best.1
}

/// Two parallel lines through the ideal ends of a knee's stems, one line a
/// side (least squares): their slope, and the line half way between them
/// at the first stem.
fn knee_fit(stems: &[QStem], infos: &[(f64, f64)], x0: f64) -> (f64, f64) {
    // y = a + s x + c u, u = 1 for the stems up: the normal equations.
    let mut m = [[0.0f64; 3]; 3];
    let mut v = [0.0f64; 3];
    for (st, i) in stems.iter().zip(infos) {
        let row = [1.0, st.x - x0, if st.dir > 0.0 { 1.0 } else { 0.0 }];
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += row[r] * row[c];
            }
            v[r] += row[r] * i.0;
        }
    }
    let det = |m: &[[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = det(&m);
    let mean = |up: bool| {
        let ys: Vec<f64> = stems.iter().zip(infos).filter(|(s, _)| (s.dir > 0.0) == up).map(|(_, i)| i.0).collect();
        ys.iter().sum::<f64>() / ys.len().max(1) as f64
    };
    if d.abs() < 1e-9 {
        // No slope to be had (one stem a side): flat, half way.
        return (0.0, (mean(true) + mean(false)) / 2.0);
    }
    let solve = |k: usize| {
        let mut mk = m;
        for r in 0..3 {
            mk[r][k] = v[r];
        }
        det(&mk) / d
    };
    let (a, s, c) = (solve(0), solve(1), solve(2));
    (s, a + c / 2.0)
}
