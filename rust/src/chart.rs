//! Dependency-free SVG line charts (and Mermaid xychart blocks) for the Markdown report.
//!
//! Design rules applied: one y-axis per chart; 2px lines with round joins; >= 8px markers with a
//! 2px surface ring; hairline solid gridlines; muted axis ink; categorical hues in fixed slot
//! order (never cycled, at most four series per chart); a legend whenever there are two or more
//! series; a single direct label per series at its last point; text in ink colours, never the
//! series colour. Gaps in a series (null points) break the line instead of interpolating.

use std::fmt::Write as _;

/// Categorical slots 1-4 (light surface) from the reference palette; fixed order.
pub const SERIES_COLORS: [&str; 4] = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"];
const SURFACE: &str = "#fcfcfb";
const INK_PRIMARY: &str = "#0b0b0b";
const INK_SECONDARY: &str = "#52514e";
const INK_MUTED: &str = "#898781";
const GRIDLINE: &str = "#e1e0d9";
const BASELINE: &str = "#c3c2b7";
const FONT: &str = "system-ui, -apple-system, 'Segoe UI', sans-serif";

pub const MAX_SERIES: usize = SERIES_COLORS.len();

#[derive(Debug, Clone)]
pub struct Series {
    pub name: String,
    pub points: Vec<Option<f64>>,
}

#[derive(Debug, Clone)]
pub struct ChartSpec {
    pub title: String,
    /// Axis unit shown as subtitle, e.g. "percent", "EUR millions", "ratio".
    pub unit_label: String,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
}

const W: f64 = 760.0;
const H: f64 = 320.0;
const ML: f64 = 64.0;
const MR: f64 = 72.0; // room for direct end labels
const MT: f64 = 44.0;
const MB_BASE: f64 = 40.0;
const LEGEND_H: f64 = 22.0;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Compact number for ticks and labels: 3 significant-ish digits, thousands separators.
pub fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "n/a".into();
    }
    let a = v.abs();
    let s = if a >= 1000.0 {
        group_thousands(&format!("{:.0}", v))
    } else if a >= 100.0 {
        format!("{:.0}", v)
    } else if a >= 10.0 {
        trim_zeros(&format!("{:.1}", v))
    } else {
        trim_zeros(&format!("{:.2}", v))
    };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

fn group_thousands(int_str: &str) -> String {
    let (sign, digits) = match int_str.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", int_str),
    };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

/// "Nice" tick domain covering [min, max] with about `n` ticks.
fn nice_ticks(min: f64, max: f64, n: usize) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (min, max);
    if (hi - lo).abs() < f64::EPSILON {
        let pad = if lo.abs() < f64::EPSILON {
            1.0
        } else {
            lo.abs() * 0.1
        };
        lo -= pad;
        hi += pad;
    }
    let raw = (hi - lo) / n as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    ((lo / step).floor() * step, (hi / step).ceil() * step, step)
}

pub fn line_chart_svg(spec: &ChartSpec) -> String {
    let series: Vec<&Series> = spec.series.iter().take(MAX_SERIES).collect();
    let n_cat = spec.categories.len().max(1);
    let values: Vec<f64> = series
        .iter()
        .flat_map(|s| s.points.iter().flatten().copied())
        .collect();
    let (mut vmin, mut vmax) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
            (a.min(v), b.max(v))
        });
    if values.is_empty() {
        vmin = 0.0;
        vmax = 1.0;
    }
    // Start the axis at zero when all values are positive and the floor is less than half the peak.
    if vmin > 0.0 && vmin < 0.5 * vmax {
        vmin = 0.0;
    }
    if vmax < 0.0 && vmax > 0.5 * vmin {
        vmax = 0.0;
    }
    let (lo, hi, step) = nice_ticks(vmin, vmax, 5);
    let legend = series.len() >= 2;
    let mb = MB_BASE + if legend { LEGEND_H } else { 0.0 };
    let plot_w = W - ML - MR;
    let plot_h = H - MT - mb;
    let x = |i: usize| -> f64 {
        if n_cat == 1 {
            ML + plot_w / 2.0
        } else {
            ML + plot_w * i as f64 / (n_cat - 1) as f64
        }
    };
    let y = |v: f64| -> f64 { MT + plot_h - (v - lo) / (hi - lo) * plot_h };

    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{W}\" height=\"{H}\" viewBox=\"0 0 {W} {H}\" role=\"img\" aria-label=\"{}\" font-family=\"{FONT}\">",
        esc(&spec.title)
    );
    let _ = write!(
        svg,
        "<rect width=\"{W}\" height=\"{H}\" fill=\"{SURFACE}\"/>"
    );
    let _ = write!(
        svg,
        "<text x=\"{ML}\" y=\"20\" font-size=\"14\" font-weight=\"600\" fill=\"{INK_PRIMARY}\">{}</text>",
        esc(&spec.title)
    );
    let _ = write!(
        svg,
        "<text x=\"{ML}\" y=\"35\" font-size=\"11\" fill=\"{INK_MUTED}\">{}</text>",
        esc(&spec.unit_label)
    );

    // Gridlines and y ticks.
    let mut t = lo;
    let mut guard = 0;
    while t <= hi + step * 1e-9 && guard < 50 {
        let yy = y(t);
        let is_zero = t.abs() < step * 1e-9;
        let _ = write!(
            svg,
            "<line x1=\"{ML}\" x2=\"{:.1}\" y1=\"{yy:.1}\" y2=\"{yy:.1}\" stroke=\"{}\" stroke-width=\"1\"/>",
            W - MR,
            if is_zero { BASELINE } else { GRIDLINE }
        );
        let _ = write!(
            svg,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"{INK_MUTED}\" text-anchor=\"end\" style=\"font-variant-numeric: tabular-nums\">{}</text>",
            ML - 8.0,
            yy + 4.0,
            esc(&fmt_num(t))
        );
        t += step;
        guard += 1;
    }
    // X labels (thin out when crowded).
    let every = (n_cat as f64 / 10.0).ceil().max(1.0) as usize;
    for (i, c) in spec.categories.iter().enumerate() {
        if i % every != 0 && i != n_cat - 1 {
            continue;
        }
        let _ = write!(
            svg,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"{INK_MUTED}\" text-anchor=\"middle\">{}</text>",
            x(i),
            MT + plot_h + 18.0,
            esc(c)
        );
    }

    // Series: broken polylines, markers with surface ring, one end label each.
    let mut end_labels: Vec<(f64, String, usize)> = Vec::new();
    for (si, s) in series.iter().enumerate() {
        let color = SERIES_COLORS[si];
        let mut run: Vec<String> = Vec::new();
        let flush = |run: &mut Vec<String>, svg: &mut String| {
            if run.len() >= 2 {
                let _ = write!(
                    svg,
                    "<polyline fill=\"none\" stroke=\"{color}\" stroke-width=\"2\" stroke-linejoin=\"round\" stroke-linecap=\"round\" points=\"{}\"/>",
                    run.join(" ")
                );
            }
            run.clear();
        };
        for (i, p) in s.points.iter().enumerate().take(n_cat) {
            match p {
                Some(v) => run.push(format!("{:.1},{:.1}", x(i), y(*v))),
                None => flush(&mut run, &mut svg),
            }
        }
        flush(&mut run, &mut svg);
        let mut last: Option<(usize, f64)> = None;
        for (i, p) in s.points.iter().enumerate().take(n_cat) {
            if let Some(v) = p {
                let _ = write!(
                    svg,
                    "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"{color}\" stroke=\"{SURFACE}\" stroke-width=\"2\"/>",
                    x(i),
                    y(*v)
                );
                last = Some((i, *v));
            }
        }
        if let Some((i, v)) = last {
            // Value only: with a legend the colour swatch carries identity; without one the title does.
            end_labels.push((y(v), fmt_num(v), i));
        }
    }
    // Push overlapping end labels apart.
    end_labels.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for k in 1..end_labels.len() {
        if end_labels[k].0 - end_labels[k - 1].0 < 13.0 {
            end_labels[k].0 = end_labels[k - 1].0 + 13.0;
        }
    }
    for (yy, label, i) in &end_labels {
        let _ = write!(
            svg,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"{INK_SECONDARY}\">{}</text>",
            x(*i) + 8.0,
            yy + 4.0,
            esc(label)
        );
    }
    if legend {
        let mut lx = ML;
        let ly = H - 10.0;
        for (si, s) in series.iter().enumerate() {
            let _ = write!(
                svg,
                "<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"10\" height=\"10\" rx=\"2\" fill=\"{}\"/>",
                ly - 9.0,
                SERIES_COLORS[si]
            );
            let _ = write!(
                svg,
                "<text x=\"{:.1}\" y=\"{ly:.1}\" font-size=\"11\" fill=\"{INK_SECONDARY}\">{}</text>",
                lx + 14.0,
                esc(&s.name)
            );
            lx += 14.0 + 6.5 * s.name.chars().count() as f64 + 18.0;
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Mermaid `xychart-beta` blocks: one per series (the beta chart has no legend, so multi-series
/// charts would be colour-only identity). Renders on GitHub, GitLab, Obsidian and VS Code.
pub fn mermaid_blocks(spec: &ChartSpec) -> String {
    let mut out = String::new();
    let cats = spec
        .categories
        .iter()
        .map(|c| format!("\"{}\"", c.replace('"', "'")))
        .collect::<Vec<_>>()
        .join(", ");
    for s in spec.series.iter().take(MAX_SERIES) {
        let vals: Vec<String> = s
            .points
            .iter()
            .map(|p| p.map(|v| format!("{v:.4}")).unwrap_or_else(|| "0".into()))
            .collect();
        let title = if spec.series.len() > 1 {
            format!("{} - {}", spec.title, s.name)
        } else {
            spec.title.clone()
        };
        let _ = writeln!(out, "```mermaid\nxychart-beta\n    title \"{}\"\n    x-axis [{}]\n    y-axis \"{}\"\n    line [{}]\n```\n",
            title.replace('"', "'"), cats, spec.unit_label.replace('"', "'"), vals.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(fmt_num(27559.0), "27,559");
        assert_eq!(fmt_num(-1234567.0), "-1,234,567");
        assert_eq!(fmt_num(123.4), "123");
        assert_eq!(fmt_num(12.34), "12.3");
        assert_eq!(fmt_num(0.8), "0.8");
        assert_eq!(fmt_num(0.0), "0");
    }

    #[test]
    fn nice_ticks_cover_domain() {
        let (lo, hi, step) = nice_ticks(11.3, 12.5, 5);
        assert!(lo <= 11.3 && hi >= 12.5 && step > 0.0);
        let (lo, hi, _) = nice_ticks(-19.0, 117.0, 5);
        assert!(lo <= -19.0 && hi >= 117.0);
    }

    #[test]
    fn svg_has_expected_marks() {
        let spec = ChartSpec {
            title: "Margins".into(),
            unit_label: "percent".into(),
            categories: vec!["FY2021".into(), "FY2022".into(), "FY2023".into()],
            series: vec![
                Series {
                    name: "Operating".into(),
                    points: vec![Some(12.0), None, Some(11.3)],
                },
                Series {
                    name: "Net".into(),
                    points: vec![Some(8.3), Some(8.0), Some(6.5)],
                },
            ],
        };
        let svg = line_chart_svg(&spec);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert_eq!(svg.matches("<circle").count(), 5);
        // first series has a gap: no polyline with 2+ points; second has one
        assert_eq!(svg.matches("<polyline").count(), 1);
        assert_eq!(svg.matches("<rect").count(), 3); // surface + 2 legend swatches
        assert!(svg.contains(">6.5<"));
    }

    #[test]
    fn mermaid_one_block_per_series() {
        let spec = ChartSpec {
            title: "Revenue".into(),
            unit_label: "EUR millions".into(),
            categories: vec!["FY2021".into(), "FY2022".into()],
            series: vec![Series {
                name: "Revenue".into(),
                points: vec![Some(1.0), Some(2.0)],
            }],
        };
        let m = mermaid_blocks(&spec);
        assert_eq!(m.matches("xychart-beta").count(), 1);
        assert!(m.contains("line [1.0000, 2.0000]"));
    }
}
