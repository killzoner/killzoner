mod graphql;

use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use time::OffsetDateTime;
use tracing::debug;

// --- CLI arguments ---

#[derive(Parser)]
#[command(name = "github-contributions")]
#[command(about = "List public repositories you've contributed to")]
struct Args {
    #[arg(long, env = "GITHUB_TOKEN")]
    token: String,

    #[arg(short, long, default_value = "killzoner")]
    username: String,

    #[arg(short, long, default_value = "template.md")]
    template: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging (use RUST_LOG=debug to see debug output)
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let client = reqwest::Client::new();

    // Filter out repos not pushed in the last 2 years
    let cutoff_year = (OffsetDateTime::now_utc().year() - 2) as u16;

    // Fetch contributions from GitHub
    let repos = graphql::fetch_repos(&client, &args.token, &args.username, cutoff_year).await?;
    debug!(count = repos.len(), "fetched repos");

    // Deduplicate repos, filter out user's own, keep latest year and sum counts
    let mut seen: HashMap<String, graphql::RepoInfo> = HashMap::new();
    for repo in repos {
        if !repo.name.starts_with(&format!("{}/", args.username)) {
            seen.entry(repo.name.clone())
                .and_modify(|existing| {
                    existing.count += 1;
                    if repo.year > existing.year {
                        existing.year = repo.year;
                    }
                })
                .or_insert(repo);
        }
    }

    // Sort by year (desc), then by stars (desc)
    let mut repos: Vec<_> = seen.into_values().collect();
    repos.sort_by(|a, b| b.year.cmp(&a.year).then(b.stars.cmp(&a.stars)));

    // --- Output markdown ---

    // Optional introduction template
    if let Ok(content) = fs::read_to_string(&args.template) {
        println!("{}\n", content.trim());
    }

    // Header
    println!("## Open Source Contributions\n");

    // Repository list
    for r in &repos {
        // Format stars as "1.2k" for thousands
        let stars = if r.stars >= 1000 {
            format!("{:.1}k", r.stars as f64 / 1000.0)
        } else {
            r.stars.to_string()
        };
        let prs = if r.count == 1 {
            "1 PR"
        } else {
            &format!("{} PRs", r.count)
        };
        println!(
            "- [{}](https://github.com/{}) ⭐{stars} · {prs} ({})",
            r.name, r.name, r.year
        );
    }

    Ok(())
}
