//! A Markdown report of a project and, when there is one, the result of its last run.

use std::fmt::Write;

use crate::domain::project::Project;
use crate::domain::server_profile::{DiskKind, OperatingSystem, Virtualization};
use crate::domain::topology::NodeKind;
use crate::domain::traffic::SpikePattern;
use crate::sim::report::{Severity, SimReport, WindowStats};
use crate::whatif::misses;

/// Shown when no run is attached, so a reader never mistakes the design for a result.
const NO_RUN_NOTE: &str =
    "No simulation result is attached. Run a simulation and export again to include one.";

pub fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Client => "Clients",
        NodeKind::LoadBalancer => "Load balancer",
        NodeKind::AppServer => "App server",
        NodeKind::Database => "Database",
        NodeKind::Cache => "Cache",
        NodeKind::Queue => "Queue",
    }
}

pub fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Healthy => "healthy",
        Severity::Warning => "busy",
        Severity::Saturated => "saturated",
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn pct(fraction: f64) -> String {
    format!("{:.1}%", fraction * 100.0)
}

fn spike(pattern: &SpikePattern) -> String {
    match pattern {
        SpikePattern::None => "none".to_owned(),
        SpikePattern::Step {
            at_s,
            multiplier,
            duration_s,
        } => format!("×{multiplier} for {duration_s} s starting at {at_s} s"),
        SpikePattern::Periodic {
            period_s,
            multiplier,
            duration_s,
        } => format!("×{multiplier} for {duration_s} s every {period_s} s"),
    }
}

fn window(w: &WindowStats) -> String {
    format!("{} errors, p95 {:.0} ms", pct(w.error_rate), w.p95_ms)
}

pub fn render(project: &Project, report: Option<&SimReport>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", project.name.trim());
    let _ = writeln!(
        out,
        "> Estimate from a simulation model, not a measurement. Treat the numbers as a guide to where a design saturates.\n"
    );
    design(&mut out, project);
    match report {
        Some(r) => results(&mut out, project, r),
        None => {
            let _ = writeln!(out, "## Results\n\n{NO_RUN_NOTE}");
        }
    }
    out
}

fn design(out: &mut String, project: &Project) {
    let s = &project.server_profile;
    let os = match s.os {
        OperatingSystem::Linux => "Linux",
        OperatingSystem::MacOs => "macOS",
        OperatingSystem::Windows => "Windows",
        OperatingSystem::Other => "other",
    };
    let disk = match s.disk.kind {
        DiskKind::Hdd => "HDD",
        DiskKind::Ssd => "SSD",
        DiskKind::Nvme => "NVMe",
    };
    let virt = match s.virtualization {
        Virtualization::BareMetal => "bare metal",
        Virtualization::Vm => "virtual machine",
        Virtualization::Container => "container",
    };
    let _ = writeln!(out, "## Server profile\n");
    let _ = writeln!(
        out,
        "Every node in the topology is one machine like this.\n"
    );
    let _ = writeln!(out, "| Setting | Value |\n| --- | --- |");
    let _ = writeln!(out, "| OS | {os} |");
    let _ = writeln!(out, "| CPU | {} cores at {} GHz |", s.cpu_cores, s.cpu_ghz);
    let _ = writeln!(out, "| RAM | {} GB |", s.ram_gb);
    let _ = writeln!(out, "| Disk | {disk}, {} IOPS |", s.disk.iops);
    let _ = writeln!(
        out,
        "| Network | {} Mbps, {} ms latency |",
        s.network.bandwidth_mbps, s.network.latency_ms
    );
    let _ = writeln!(out, "| Virtualization | {virt} |");
    let _ = writeln!(out, "| File descriptor limit | {} |\n", s.fd_limit);

    let t = &project.traffic_profile;
    let _ = writeln!(out, "## Traffic\n");
    let _ = writeln!(out, "| Setting | Value |\n| --- | --- |");
    let _ = writeln!(out, "| Concurrent users | {} |", t.concurrent_users);
    let _ = writeln!(out, "| Requests per second | {} |", t.requests_per_second);
    let _ = writeln!(
        out,
        "| Reads / writes | {} / {} |",
        pct(t.read_ratio),
        pct(1.0 - t.read_ratio)
    );
    let _ = writeln!(out, "| Spike | {} |\n", spike(&t.spike));

    let _ = writeln!(out, "## Topology\n");
    let _ = writeln!(out, "| Node | Type | Replicas |\n| --- | --- | --- |");
    for n in &project.topology.nodes {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            cell(&n.label),
            kind_name(n.kind),
            n.replicas
        );
    }
    let label_of = |id: &str| {
        project
            .topology
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map_or_else(|| id.to_owned(), |n| n.label.clone())
    };
    if !project.topology.edges.is_empty() {
        let _ = writeln!(out, "\nConnections:\n");
        for e in &project.topology.edges {
            let _ = writeln!(
                out,
                "- {} → {}",
                cell(&label_of(&e.source)),
                cell(&label_of(&e.target))
            );
        }
    }
    let p = &project.chaos.policy;
    let _ = writeln!(
        out,
        "\nClient policy: timeout {} ms, {} retries, {} ms backoff.",
        p.timeout_ms, p.max_retries, p.backoff_ms
    );
    let slo = &project.whatif.slo;
    let _ = writeln!(
        out,
        "\nTargets: p95 at most {} ms and errors at most {}.\n",
        slo.p95_ms,
        pct(slo.max_error_rate)
    );
}

fn results(out: &mut String, project: &Project, r: &SimReport) {
    let _ = writeln!(out, "## Results\n");
    let _ = writeln!(
        out,
        "Seed {}, {} s simulated with {} s warm-up.\n",
        r.seed, r.duration_s, r.warmup_s
    );
    let misses = misses(&r.summary, &project.whatif.slo);
    if misses.is_empty() {
        let _ = writeln!(out, "**Meets the targets.**\n");
    } else {
        let _ = writeln!(out, "**Misses the targets:** {}.\n", misses.join(", "));
    }
    let s = &r.summary;
    let _ = writeln!(out, "| Metric | Value |\n| --- | --- |");
    let _ = writeln!(out, "| Requests offered | {} |", s.offered);
    let _ = writeln!(out, "| Completed | {} |", s.completed);
    let _ = writeln!(out, "| Failed | {} ({}) |", s.errors, pct(s.error_rate));
    let _ = writeln!(out, "| Retries sent | {} |", s.retries);
    let _ = writeln!(out, "| Throughput | {:.1} req/s |", s.throughput_rps);
    let _ = writeln!(
        out,
        "| Latency | mean {:.1} ms, p50 {:.1} ms, p95 {:.1} ms, p99 {:.1} ms |\n",
        s.latency.mean_ms, s.latency.p50_ms, s.latency.p95_ms, s.latency.p99_ms
    );

    if let Some(b) = &r.bottleneck {
        let _ = writeln!(
            out,
            "**Bottleneck:** {} ({}): {}.\n",
            cell(&b.label),
            severity_name(b.severity),
            b.reason
        );
    }

    let _ = writeln!(out, "### Nodes\n");
    let _ = writeln!(
        out,
        "| Node | Status | Busy | Mean queue | p95 in node | Rejected | Timed out | Lost to faults |\n| --- | --- | --- | --- | --- | --- | --- | --- |"
    );
    for n in &r.nodes {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {:.1} | {:.1} ms | {} | {} | {} |",
            cell(&n.label),
            severity_name(n.severity),
            pct(n.utilization),
            n.mean_queue,
            n.latency.p95_ms,
            n.rejected,
            n.timed_out,
            n.faulted
        );
    }

    if !r.faults.is_empty() {
        let _ = writeln!(out, "\n### Faults\n");
        let _ = writeln!(
            out,
            "| Fault | Window | Before | During | After | Recovery |\n| --- | --- | --- | --- | --- | --- |"
        );
        for f in &r.faults {
            let recovery = f
                .recovery_s
                .map_or_else(|| "not within the run".to_owned(), |s| format!("{s:.0} s"));
            let _ = writeln!(
                out,
                "| {} | {:.0}–{:.0} s | {} | {} | {} | {recovery} |",
                cell(&f.label),
                f.start_s,
                f.end_s,
                window(&f.before),
                window(&f.during),
                window(&f.after),
            );
        }
    }

    if !r.controls.is_empty() {
        let _ = writeln!(out, "\n### Live controls\n");
        let _ = writeln!(
            out,
            "This run was steered while it played. Replaying these commands reproduces it.\n"
        );
        for c in &r.controls {
            let _ = writeln!(out, "- at {} s: {}", c.t_s, c.summary);
        }
    }

    if !r.assumptions.is_empty() {
        let _ = writeln!(out, "\n### Assumptions\n");
        for a in &r.assumptions {
            let _ = writeln!(out, "- {a}");
        }
    }
    let _ = writeln!(out, "\n_{}_", r.notice);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{SimOptions, run};

    fn project() -> Project {
        let mut p = Project::new("Shop | v2");
        p.traffic_profile.requests_per_second = 60.0;
        p
    }

    #[test]
    fn a_design_without_a_run_says_so() {
        let text = render(&project(), None);
        assert!(text.starts_with("# Shop | v2"));
        assert!(text.contains("Estimate from a simulation model"));
        assert!(text.contains(NO_RUN_NOTE));
        assert!(text.contains("| Requests per second | 60 |"));
        assert!(text.contains("Connections:"));
    }

    #[test]
    fn a_run_adds_the_verdict_and_per_node_results() {
        let p = project();
        let opts = SimOptions {
            seed: 1,
            duration_s: 20.0,
            warmup_s: 2.0,
        };
        let report = run(&p, &opts).expect("run");
        let text = render(&p, Some(&report));
        assert!(text.contains("**Meets the targets.**") || text.contains("**Misses the targets:"));
        assert!(text.contains("### Nodes"));
        assert!(text.contains(&report.notice));
        for n in &p.topology.nodes {
            if n.kind != NodeKind::Client {
                assert!(text.contains(&n.label), "missing {}", n.label);
            }
        }
    }

    #[test]
    fn live_controls_are_listed_with_their_time() {
        use crate::sim::control::{ControlCommand, ControlRecord};
        let p = project();
        let opts = SimOptions {
            seed: 1,
            duration_s: 10.0,
            warmup_s: 2.0,
        };
        let mut report = run(&p, &opts).expect("run");
        assert!(!render(&p, Some(&report)).contains("Live controls"));
        report.controls.push(ControlRecord {
            t_s: 4,
            command: ControlCommand::SetTraffic { multiplier: 3.0 },
            summary: "Traffic \u{d7}3".to_owned(),
            fault_id: None,
        });
        let text = render(&p, Some(&report));
        assert!(text.contains("### Live controls"));
        assert!(text.contains("- at 4 s: Traffic \u{d7}3"));
    }

    #[test]
    fn pipes_in_names_cannot_break_tables() {
        assert_eq!(cell("a|b\nc"), "a\\|b c");
    }
}
