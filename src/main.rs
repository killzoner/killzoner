mod graphql;

use clap::Parser;
use handlebars::Handlebars;
use serde::Serialize;
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

    #[arg(short, long, default_value = "template.hbs")]
    template: PathBuf,

    #[arg(
        long = "exclude-personal-repos",
        value_delimiter = ',',
        default_value = "killzoner,cubejs-prometheus"
    )]
    exclude_personal_repos: Vec<String>,
}

// --- Display structs for template ---

#[derive(Serialize)]
struct TemplateContext {
    active_repos: Vec<ActiveRepoDisplay>,
    contributions: Vec<ContributionDisplay>,
}

#[derive(Serialize)]
struct ActiveRepoDisplay {
    name: String,
    url: String,
    stars: String,
    description: Option<String>,
}

#[derive(Serialize)]
struct ContributionDisplay {
    name: String,
    url: String,
    stars: String,
    pr_label: String,
    year: u16,
}

// --- Helper functions ---

fn format_stars(stars: u32) -> String {
    if stars >= 1000 {
        format!("{:.1}k", stars as f64 / 1000.0)
    } else {
        stars.to_string()
    }
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

    // Fetch contributions and active repos in parallel
    let (repos, active_repos) = tokio::try_join!(
        graphql::fetch_repos(&client, &args.token, &args.username, cutoff_year),
        graphql::fetch_active_repos(
            &client,
            &args.token,
            &args.username,
            cutoff_year,
            &args.exclude_personal_repos
        )
    )?;
    debug!(
        contributions = repos.len(),
        active = active_repos.len(),
        "fetched repos"
    );

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

    // Convert to display structs
    let contributions: Vec<ContributionDisplay> = repos
        .into_iter()
        .map(|r| ContributionDisplay {
            url: format!("https://github.com/{}", r.name),
            name: r.name,
            stars: format_stars(r.stars),
            pr_label: if r.count == 1 {
                "1 PR".to_string()
            } else {
                format!("{} PRs", r.count)
            },
            year: r.year,
        })
        .collect();

    let mut active_repos: Vec<ActiveRepoDisplay> = active_repos
        .into_iter()
        .map(|r| ActiveRepoDisplay {
            name: r.name,
            url: r.url,
            stars: format_stars(r.stars),
            description: r.description,
        })
        .collect();
    active_repos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Build template context
    let context = TemplateContext {
        active_repos,
        contributions,
    };

    // Load and render template
    let template_content = fs::read_to_string(&args.template)?;
    let mut handlebars = Handlebars::new();
    handlebars.register_template_string("readme", &template_content)?;

    let output = handlebars.render("readme", &context)?;
    print!("{}", output);

    Ok(())
}
