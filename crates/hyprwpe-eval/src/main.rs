use clap::Parser;
use crossbeam_channel::unbounded;
use rayon::prelude::*;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use hyprwpe_core::{ConfigManager, WallpaperType};
use hyprwpe_eval::evaluator::{
    detect_wallpaper_info, evaluate_wallpaper_with_options, EvaluationMode, EvaluationOptions,
    EvaluationReport,
};

#[derive(Parser)]
#[command(name = "hyprwpe-eval")]
#[command(about = "HyprWpE Multi-Engine Visual Inspector & Quality Scoring Suite", long_about = None)]
struct Cli {
    /// Single Wallpaper ID to inspect
    #[arg(short, long)]
    id: Option<String>,

    /// Run evaluation across Steam Workshop wallpapers
    #[arg(short, long)]
    all: bool,

    /// Filter wallpaper type: all, scene, video, web
    #[arg(short = 't', long, default_value = "all")]
    filter_type: String,

    /// Run in Live Desktop mode on active Hyprland desktop (capturing monitor via grim)
    #[arg(long)]
    live: bool,

    /// Monitor to target in Live Desktop mode (e.g. DP-2, HDMI-A-1)
    #[arg(short = 'm', long)]
    monitor: Option<String>,

    /// Keep wallpaper active on desktop after live inspection
    #[arg(long)]
    keep: bool,

    /// Maximum wallpapers to inspect when --all is set (0 = inspect all)
    #[arg(short, long, default_value_t = 10)]
    limit: usize,

    /// Number of wallpapers to skip before starting inspection when --all is set
    #[arg(long, default_value_t = 0)]
    offset: usize,

    /// Number of parallel evaluation worker threads (Headless mode)
    #[arg(short, long, default_value_t = 4)]
    jobs: usize,

    /// Output directory for visual inspection collages and reports
    #[arg(short, long, default_value = "/tmp/hyprwpe_eval")]
    out: PathBuf,
}

fn matches_filter(wp_path: &Path, filter: &str) -> bool {
    let f = filter.to_lowercase();
    if f == "all" {
        return true;
    }
    let (t, _, _) = detect_wallpaper_info(wp_path);
    match (t, f.as_str()) {
        (WallpaperType::Scene, "scene") => true,
        (WallpaperType::Video, "video") => true,
        (WallpaperType::Web, "web") => true,
        _ => false,
    }
}

fn format_duration(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

fn generate_markdown_scorecard(reports: &[EvaluationReport], out_path: &Path, duration: u64) {
    let total = reports.len();
    let passed = reports.iter().filter(|r| r.final_score >= 70.0).count();
    let avg_score = if total > 0 {
        reports.iter().map(|r| r.final_score).sum::<f32>() / total as f32
    } else {
        0.0
    };

    let mut md = String::new();
    md.push_str("# HyprWpE Visual Parity Scorecard\n\n");
    md.push_str(&format!("- **Total Evaluated**: {total}\n"));
    md.push_str(&format!(
        "- **Passed (\u{2265} 70.0)**: {passed} ({:.1}%)\n",
        (passed as f32 / total.max(1) as f32) * 100.0
    ));
    md.push_str(&format!("- **Average Quality Score**: {:.1} / 100\n", avg_score));
    md.push_str(&format!("- **Total Test Duration**: {}\n\n", format_duration(duration)));

    md.push_str("## Detailed Results\n\n");
    md.push_str("| # | ID | Type | Mode | Score | Color Sim | Structure | Motion | Title | Status |\n");
    md.push_str("|---|---|---|---|---|---|---|---|---|---|\n");

    for (idx, r) in reports.iter().enumerate() {
        let status = if r.final_score >= 70.0 {
            "**PASS**"
        } else if r.final_score >= 50.0 {
            "WARN"
        } else {
            "FAIL"
        };

        md.push_str(&format!(
            "| {:02} | `{}` | {} | {} | **{:.1}** | {:.1}% | {:.1}% | {:.1}% | {} | {} |\n",
            idx + 1,
            r.wallpaper_id,
            r.wallpaper_type,
            r.mode,
            r.final_score,
            r.color_score,
            r.struct_score,
            r.motion_score,
            r.title,
            status
        ));
    }

    md.push_str("\n## Visual Comparison Collages\n\n");
    for r in reports {
        if let Some(ref col) = r.collage_path {
            md.push_str(&format!(
                "### [{}] {} (ID: `{}`)\n",
                r.wallpaper_type, r.title, r.wallpaper_id
            ));
            md.push_str(&format!(
                "**Final Score**: {:.1} / 100 | **Mode**: {}\n\n",
                r.final_score, r.mode
            ));
            md.push_str(&format!("![Comparison]({col})\n\n"));
        }
    }

    let _ = fs::write(out_path, md);
}

fn main() {
    let cli = Cli::parse();
    let steam_dir = ConfigManager::default_wallpaper_dir();

    let options = EvaluationOptions {
        mode: if cli.live {
            EvaluationMode::LiveDesktop
        } else {
            EvaluationMode::Headless
        },
        live_monitor: cli.monitor.clone(),
        keep_running: cli.keep,
    };

    if let Some(id) = cli.id {
        println!(
            "=== Running HyprWpE Visual Inspection on Wallpaper {id} [Mode: {:?}] ===",
            options.mode
        );
        io::stdout().flush().ok();
        let report = evaluate_wallpaper_with_options(&id, &steam_dir, &cli.out, &options);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        println!("\nFinal Quality Score: {:.1} / 100", report.final_score);
        if let Some(collage) = &report.collage_path {
            println!("Comparison Collage saved to: {collage}");
        }
        return;
    }

    if cli.all {
        if !steam_dir.is_dir() {
            eprintln!("Error: Steam Workshop directory not found at {}", steam_dir.display());
            std::process::exit(1);
        }

        let mut wallpapers = Vec::new();
        if let Ok(entries) = fs::read_dir(&steam_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && matches_filter(&p, &cli.filter_type) {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        wallpapers.push(name.to_string());
                    }
                }
            }
        }
        wallpapers.sort();

        let total_found = wallpapers.len();
        if cli.offset > 0 {
            if cli.offset < wallpapers.len() {
                wallpapers = wallpapers.split_off(cli.offset);
            } else {
                wallpapers.clear();
            }
        }
        if cli.limit > 0 && wallpapers.len() > cli.limit {
            wallpapers.truncate(cli.limit);
        }

        let total = wallpapers.len();
        let effective_jobs = if cli.live { 1 } else { cli.jobs };

        println!(
            "=== Running HyprWpE Visual Inspection Suite on {} Wallpapers (Filter: {}, Found: {}, Mode: {:?}, Jobs: {}) ===",
            total, cli.filter_type, total_found, options.mode, effective_jobs
        );
        println!(
            "{:<4} | {:<12} | {:<6} | {:<7} | {:<6} | {:<6} | {:<11} | {:<25} | {:<7}",
            "#", "ID", "Type", "Score", "Color", "Motion", "Artifacts", "Title", "Time"
        );
        println!("{}", "-".repeat(100));
        io::stdout().flush().ok();

        let start_time = Instant::now();
        let mut reports = Vec::new();

        if cli.live {
            // Live desktop mode runs sequentially
            for (idx, wid) in wallpapers.iter().enumerate() {
                let wp_start = Instant::now();
                let rep = evaluate_wallpaper_with_options(wid, &steam_dir, &cli.out, &options);
                let duration = wp_start.elapsed().as_secs_f32();

                let artifact_status = if rep.solid_color_fallback {
                    "SOLID_COLOR"
                } else if rep.oscillating_vertical_lines {
                    "VERT_STRIPES"
                } else {
                    "CLEAN"
                };

                let title_trunc: String = if rep.title.chars().count() > 25 {
                    format!("{}...", rep.title.chars().take(22).collect::<String>())
                } else {
                    rep.title.clone()
                };

                println!(
                    "[{:>2}/{:<2}] {:<12} | {:<6} | {:<7.1} | {:<6.1} | {:<6.1} | {:<11} | {:<25} | [{:.1}s]",
                    idx + 1, total, rep.wallpaper_id, rep.wallpaper_type, rep.final_score, rep.color_score, rep.motion_score, artifact_status, title_trunc, duration
                );
                io::stdout().flush().ok();
                reports.push(rep);
            }
        } else {
            // Headless mode runs in parallel thread pool
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(effective_jobs)
                .build()
                .unwrap();

            let counter = Arc::new(AtomicUsize::new(0));
            let (tx, rx) = unbounded::<(EvaluationReport, f32)>();

            let steam_dir_ref = steam_dir.clone();
            let out_ref = cli.out.clone();
            let options_ref = options.clone();

            pool.spawn(move || {
                wallpapers.into_par_iter().for_each(|wid| {
                    let wp_start = Instant::now();
                    let rep = evaluate_wallpaper_with_options(&wid, &steam_dir_ref, &out_ref, &options_ref);
                    let duration = wp_start.elapsed().as_secs_f32();
                    let _ = tx.send((rep, duration));
                });
            });

            while let Ok((rep, duration)) = rx.recv() {
                let done = counter.fetch_add(1, Ordering::SeqCst) + 1;
                let elapsed_secs = start_time.elapsed().as_secs();

                let artifact_status = if rep.solid_color_fallback {
                    "SOLID_COLOR"
                } else if rep.oscillating_vertical_lines {
                    "VERT_STRIPES"
                } else {
                    "CLEAN"
                };

                let title_trunc: String = if rep.title.chars().count() > 25 {
                    format!("{}...", rep.title.chars().take(22).collect::<String>())
                } else {
                    rep.title.clone()
                };

                let eta_str = if done > 1 && done < total {
                    let avg = elapsed_secs as f64 / done as f64;
                    let rem = (avg * (total - done) as f64) as u64;
                    format!(" (ETA: {})", format_duration(rem))
                } else {
                    "".to_string()
                };

                println!(
                    "[{:>2}/{:<2}] {:<12} | {:<6} | {:<7.1} | {:<6.1} | {:<6.1} | {:<11} | {:<25} | [{:.1}s]{}",
                    done, total, rep.wallpaper_id, rep.wallpaper_type, rep.final_score, rep.color_score, rep.motion_score, artifact_status, title_trunc, duration, eta_str
                );
                io::stdout().flush().ok();

                reports.push(rep);
            }
        }

        let total_elapsed = start_time.elapsed().as_secs();
        let _ = fs::create_dir_all(&cli.out);
        let scorecard_json_path = cli.out.join("scorecard.json");
        let _ = fs::write(
            &scorecard_json_path,
            serde_json::to_string_pretty(&reports).unwrap_or_default(),
        );

        let scorecard_md_path = cli.out.join("scorecard.md");
        generate_markdown_scorecard(&reports, &scorecard_md_path, total_elapsed);

        println!(
            "\nVisual Inspection Suite Completed in {}!\nScorecard JSON: {}\nScorecard Markdown: {}",
            format_duration(total_elapsed),
            scorecard_json_path.display(),
            scorecard_md_path.display()
        );
        return;
    }

    println!("Please specify --id <WALLPAPER_ID> or --all");
}
