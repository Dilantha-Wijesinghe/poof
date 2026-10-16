//! Prints a quick storage report from the terminal, without the GUI.
//!
//! ```sh
//! cargo run --release -p poof-core --example report -- ~/Developer
//! ```

use std::path::PathBuf;
use std::time::Instant;

use poof_core::size::ByteSize;
use poof_core::{cleanup, paths, scan, suspects, system, volume};

fn main() {
    let root = std::env::args().nth(1).map_or_else(paths::home, PathBuf::from);

    let disk = volume::query(&root).expect("volume info");
    println!(
        "Disk: {} total · {} available ({} purgeable)",
        ByteSize(disk.total),
        ByteSize(disk.available),
        ByteSize(disk.purgeable())
    );
    println!("Full Disk Access: {}", system::has_full_disk_access());
    if let Ok(snaps) = system::snapshots() {
        println!("Local snapshots: {}", snaps.len());
    }

    let started = Instant::now();
    let tree = scan::scan(&root, &scan::Options::default(), &scan::Progress::default()).expect("scan");
    let top = tree.get(tree.root());
    println!(
        "\nScanned {} in {:.1?}: {} in {} files ({} nodes, {} unreadable folders)",
        paths::display(&root),
        started.elapsed(),
        ByteSize(top.size),
        top.files,
        tree.len(),
        tree.unreadable
    );
    for (_, child) in tree.children(tree.root()).take(10) {
        println!("  {:>10}  {}", ByteSize(child.size).to_string(), child.name);
    }

    let started = Instant::now();
    let findings = cleanup::find(cleanup::CATALOG, cleanup::Options { stale_project_days: 30 });
    println!("\nReclaimable ({:.1?}):", started.elapsed());
    for f in &findings {
        println!("  {:>10}  {} ({} items)", ByteSize(f.total()).to_string(), f.rule.name, f.items.len());
        for item in f.items.iter().take(3) {
            println!("  {:>10}      {}", ByteSize(item.size).to_string(), paths::display(&item.path));
        }
    }

    let started = Instant::now();
    let measured = suspects::measure();
    println!("\nUsual suspects ({:.1?}):", started.elapsed());
    for m in measured {
        println!("  {:>10}  {}", ByteSize(m.size).to_string(), m.suspect.name);
    }
}
