//! A static SVG diagram of the topology, coloured by node status when a run is attached.

use std::fmt::Write;

use super::markdown::{kind_name, severity_name};
use crate::domain::topology::Topology;
use crate::sim::report::{Severity, SimReport};

const NODE_W: f64 = 168.0;
const NODE_H: f64 = 64.0;
const MARGIN: f64 = 40.0;
const TITLE_H: f64 = 34.0;

const INK: &str = "#1d2330";
const MUTED: &str = "#5d6678";
const LINE: &str = "#9aa3b5";
const NEUTRAL: &str = "#8e96a8";
const HEALTHY: &str = "#2f9e75";
const WARNING: &str = "#c9971f";
const SATURATED: &str = "#d65353";

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

fn colour(severity: Option<Severity>) -> &'static str {
    match severity {
        None => NEUTRAL,
        Some(Severity::Healthy) => HEALTHY,
        Some(Severity::Warning) => WARNING,
        Some(Severity::Saturated) => SATURATED,
    }
}

/// Renders the topology. With a report, each node shows how busy it was and its status.
pub fn render(title: &str, topology: &Topology, report: Option<&SimReport>) -> String {
    let min_x = topology
        .nodes
        .iter()
        .map(|n| n.position.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = topology
        .nodes
        .iter()
        .map(|n| n.position.y)
        .fold(f64::INFINITY, f64::min);
    let (min_x, min_y) = if topology.nodes.is_empty() {
        (0.0, 0.0)
    } else {
        (min_x, min_y)
    };
    let place = |x: f64, y: f64| (x - min_x + MARGIN, y - min_y + MARGIN + TITLE_H);
    let max_x = topology
        .nodes
        .iter()
        .map(|n| place(n.position.x, n.position.y).0 + NODE_W)
        .fold(MARGIN + NODE_W, f64::max);
    let max_y = topology
        .nodes
        .iter()
        .map(|n| place(n.position.x, n.position.y).1 + NODE_H)
        .fold(MARGIN + TITLE_H + NODE_H, f64::max);
    let (w, h) = (max_x + MARGIN, max_y + MARGIN);

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">"##
    );
    let _ = writeln!(svg, "<title>{}</title>", escape(title));
    let _ = writeln!(
        svg,
        r##"<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="{LINE}"/></marker></defs>"##
    );
    let _ = writeln!(
        svg,
        r##"<rect width="100%" height="100%" fill="#ffffff"/>"##
    );
    let _ = writeln!(
        svg,
        r##"<text x="{MARGIN}" y="{:.0}" font-size="18" font-weight="600" fill="{INK}">{}</text>"##,
        MARGIN - 6.0,
        escape(title)
    );

    for e in &topology.edges {
        let from = topology.nodes.iter().find(|n| n.id == e.source);
        let to = topology.nodes.iter().find(|n| n.id == e.target);
        if let (Some(a), Some(b)) = (from, to) {
            let (ax, ay) = place(a.position.x, a.position.y);
            let (bx, by) = place(b.position.x, b.position.y);
            let _ = writeln!(
                svg,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{LINE}" stroke-width="1.5" marker-end="url(#arrow)"/>"##,
                ax + NODE_W,
                ay + NODE_H / 2.0,
                bx,
                by + NODE_H / 2.0
            );
        }
    }

    for n in &topology.nodes {
        let (x, y) = place(n.position.x, n.position.y);
        let result = report.and_then(|r| r.nodes.iter().find(|r| r.id == n.id));
        let stroke = colour(result.map(|r| r.severity));
        let name = if n.replicas > 1 {
            format!("{} ×{}", n.label, n.replicas)
        } else {
            n.label.clone()
        };
        let detail = match result {
            Some(r) => format!(
                "{:.0}% busy, {}",
                r.utilization * 100.0,
                severity_name(r.severity)
            ),
            None => kind_name(n.kind).to_owned(),
        };
        let _ = writeln!(
            svg,
            r##"<g><rect x="{x:.1}" y="{y:.1}" width="{NODE_W}" height="{NODE_H}" rx="8" fill="#ffffff" stroke="{stroke}" stroke-width="2.5"/><text x="{:.1}" y="{:.1}" font-size="14" font-weight="600" fill="{INK}" text-anchor="middle">{}</text><text x="{:.1}" y="{:.1}" font-size="12" fill="{MUTED}" text-anchor="middle">{}</text></g>"##,
            x + NODE_W / 2.0,
            y + 27.0,
            escape(&name),
            x + NODE_W / 2.0,
            y + 47.0,
            escape(&detail)
        );
    }
    if report.is_some() {
        let _ = writeln!(
            svg,
            r##"<text x="{MARGIN}" y="{:.0}" font-size="11" fill="{MUTED}">Estimate from a simulation model, not a measurement.</text>"##,
            h - 14.0
        );
    }
    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::sim::{SimOptions, run};

    #[test]
    fn draws_every_node_and_edge() {
        let p = Project::new("Demo");
        let svg = render("Demo", &p.topology, None);
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert_eq!(svg.matches("<line").count(), p.topology.edges.len());
        for n in &p.topology.nodes {
            assert!(svg.contains(&n.label));
        }
        assert!(!svg.contains("Estimate from a simulation model"));
    }

    #[test]
    fn colours_follow_the_run_and_the_note_is_added() {
        let mut p = Project::new("Overloaded");
        p.traffic_profile.requests_per_second = 20_000.0;
        let opts = SimOptions {
            seed: 1,
            duration_s: 15.0,
            warmup_s: 2.0,
        };
        let report = run(&p, &opts).expect("run");
        let svg = render("Overloaded", &p.topology, Some(&report));
        assert!(svg.contains(SATURATED));
        assert!(svg.contains("busy"));
        assert!(svg.contains("Estimate from a simulation model"));
    }

    #[test]
    fn escapes_markup_in_labels() {
        let mut p = Project::new("x");
        p.topology.nodes[1].label = "<script>\"a&b\"</script>".to_owned();
        let svg = render("t <b>", &p.topology, None);
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("&lt;script&gt;&quot;a&amp;b&quot;"));
        assert!(svg.contains("t &lt;b&gt;"));
    }

    #[test]
    fn an_empty_topology_still_renders() {
        let svg = render("Empty", &Topology::default(), None);
        assert!(svg.contains("<svg"));
    }
}
