use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

// --- Request types (sent to GitHub) ---

#[derive(Serialize)]
struct Request {
    query: &'static str,
    variables: Variables,
}

#[derive(Serialize)]
struct Variables {
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

// --- Response types (received from GitHub) ---

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
    errors: Option<Vec<Error>>,
}

#[derive(Deserialize)]
struct Error {
    message: String,
}

#[derive(Deserialize)]
struct Data {
    user: Option<User>,
}

#[derive(Deserialize)]
struct User {
    #[serde(rename = "pullRequests")]
    pull_requests: PullRequests,
}

#[derive(Deserialize)]
struct PullRequests {
    nodes: Vec<PullRequestNode>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Deserialize)]
struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
struct PullRequestNode {
    repository: Repository,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    #[serde(rename = "isPrivate")]
    is_private: bool,
    #[serde(rename = "isArchived")]
    is_archived: bool,
    #[serde(rename = "stargazerCount")]
    stars: u32,
    #[serde(rename = "pushedAt")]
    pushed_at: Option<String>,
}

// --- Public output types ---

#[derive(Clone, Serialize)]
pub struct RepoInfo {
    pub name: String,
    pub stars: u32,
    pub year: u16,
    pub count: u32,
}

#[derive(Clone, Serialize)]
pub struct ActiveRepo {
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub stars: u32,
}

// --- GraphQL query with pagination ---
const QUERY: &str = r#"
query($username: String!, $cursor: String) {
  user(login: $username) {
    pullRequests(first: 100, states: [MERGED], after: $cursor, orderBy: {field: CREATED_AT, direction: DESC}) {
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        mergedAt
        repository {
          nameWithOwner
          isPrivate
          isArchived
          stargazerCount
          pushedAt
        }
      }
    }
  }
}
"#;

/// Fetches all repos the user contributed to via merged PRs (with pagination).
/// Filters out: private, archived, 0-star, and inactive repos.
pub async fn fetch_repos(
    client: &reqwest::Client,
    token: &str,
    username: &str,
    cutoff_year: u16,
) -> Result<Vec<RepoInfo>, reqwest::Error> {
    let mut all_repos = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let resp: Response = client
            .post("https://api.github.com/graphql")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(USER_AGENT, "github-contributions-rust")
            .json(&Request {
                query: QUERY,
                variables: Variables {
                    username: username.into(),
                    cursor: cursor.clone(),
                },
            })
            .send()
            .await?
            .json()
            .await?;

        if let Some(errors) = resp.errors {
            for e in errors {
                tracing::debug!("GraphQL error: {}", e.message);
            }
        }

        let Some(data) = resp.data else { break };
        let Some(user) = data.user else { break };

        let page_info = user.pull_requests.page_info;

        // Filter and collect repos from this page
        for pr in user.pull_requests.nodes {
            let r = &pr.repository;
            let pushed_year: u16 = r
                .pushed_at
                .as_ref()
                .and_then(|s| s.get(..4)?.parse().ok())
                .unwrap_or(0);

            if !r.is_private && !r.is_archived && r.stars > 0 && pushed_year >= cutoff_year {
                all_repos.push(RepoInfo {
                    name: pr.repository.name_with_owner,
                    stars: pr.repository.stars,
                    year: pr
                        .merged_at
                        .as_ref()
                        .and_then(|s| s.get(..4)?.parse().ok())
                        .unwrap_or(0),
                    count: 1,
                });
            }
        }

        // Continue to next page or exit
        if page_info.has_next_page {
            cursor = page_info.end_cursor;
        } else {
            break;
        }
    }

    Ok(all_repos)
}

// --- User repositories query ---

const USER_REPOS_QUERY: &str = r#"
query($username: String!) {
  user(login: $username) {
    repositories(first: 100, ownerAffiliations: [OWNER], orderBy: {field: STARGAZERS, direction: DESC}, privacy: PUBLIC) {
      nodes {
        name
        url
        description
        isArchived
        isFork
        stargazerCount
        pushedAt
      }
    }
  }
}
"#;

#[derive(Deserialize)]
struct UserReposResponse {
    data: Option<UserReposData>,
    errors: Option<Vec<Error>>,
}

#[derive(Deserialize)]
struct UserReposData {
    user: Option<UserRepos>,
}

#[derive(Deserialize)]
struct UserRepos {
    repositories: Repositories,
}

#[derive(Deserialize)]
struct Repositories {
    nodes: Vec<RepoNode>,
}

#[derive(Deserialize)]
struct RepoNode {
    name: String,
    url: String,
    description: Option<String>,
    #[serde(rename = "isArchived")]
    is_archived: bool,
    #[serde(rename = "isFork")]
    is_fork: bool,
    #[serde(rename = "stargazerCount")]
    stars: u32,
    #[serde(rename = "pushedAt")]
    pushed_at: Option<String>,
}

/// Fetches the user's own active repositories (public, not archived, not forked, pushed within cutoff).
pub async fn fetch_active_repos(
    client: &reqwest::Client,
    token: &str,
    username: &str,
    cutoff_year: u16,
    exclusions: &[String],
) -> Result<Vec<ActiveRepo>, reqwest::Error> {
    let resp: UserReposResponse = client
        .post("https://api.github.com/graphql")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(USER_AGENT, "github-contributions-rust")
        .json(&Request {
            query: USER_REPOS_QUERY,
            variables: Variables {
                username: username.into(),
                cursor: None,
            },
        })
        .send()
        .await?
        .json()
        .await?;

    if let Some(errors) = resp.errors {
        for e in errors {
            tracing::debug!("GraphQL error: {}", e.message);
        }
    }

    let repos = resp
        .data
        .and_then(|d| d.user)
        .map(|u| u.repositories.nodes)
        .unwrap_or_default();

    let active_repos: Vec<ActiveRepo> = repos
        .into_iter()
        .filter(|r| {
            let pushed_year: u16 = r
                .pushed_at
                .as_ref()
                .and_then(|s| s.get(..4)?.parse().ok())
                .unwrap_or(0);
            !r.is_archived
                && !r.is_fork
                && pushed_year >= cutoff_year
                && !exclusions.contains(&r.name)
        })
        .map(|r| ActiveRepo {
            name: r.name,
            url: r.url,
            description: r.description,
            stars: r.stars,
        })
        .collect();

    Ok(active_repos)
}

// --- Included contributions (whitelisted repos) ---

#[derive(Deserialize)]
struct CommitSearch {
    items: Vec<CommitHit>,
}

#[derive(Deserialize)]
struct CommitHit {
    sha: String,
}

#[derive(Deserialize)]
struct AssociatedPull {
    number: u32,
    #[serde(rename = "merged_at")]
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct RepoMeta {
    #[serde(rename = "stargazers_count")]
    stars: u32,
    archived: bool,
    private: bool,
}

/// Fetches contributions for whitelisted repos whose commits landed on the default
/// branch via a merged PR (e.g. a closed PR rebased into a maintainer's release).
pub async fn fetch_included_contributions(
    client: &reqwest::Client,
    token: &str,
    username: &str,
    repos: &[String],
    cutoff_year: u16,
) -> Result<Vec<RepoInfo>, reqwest::Error> {
    let mut out = Vec::new();

    for repo in repos {
        let search: CommitSearch = client
            .get(format!(
                "https://api.github.com/search/commits?q=author:{username}+repo:{repo}"
            ))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(USER_AGENT, "github-contributions-rust")
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await?
            .json()
            .await?;

        if search.items.is_empty() {
            continue;
        }

        let meta: RepoMeta = client
            .get(format!("https://api.github.com/repos/{repo}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(USER_AGENT, "github-contributions-rust")
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await?
            .json()
            .await?;

        if meta.private || meta.archived || meta.stars == 0 {
            continue;
        }

        // Resolve commits to their merged PRs; dedup so squashed commits count once.
        let mut seen_prs = std::collections::HashSet::new();
        for hit in &search.items {
            let pulls: Vec<AssociatedPull> = client
                .get(format!(
                    "https://api.github.com/repos/{repo}/commits/{}/pulls",
                    hit.sha
                ))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header(USER_AGENT, "github-contributions-rust")
                .header(ACCEPT, "application/vnd.github+json")
                .send()
                .await?
                .json()
                .await?;

            for pull in pulls {
                let Some(merged_at) = pull.merged_at.as_deref() else {
                    continue;
                };
                if !seen_prs.insert(pull.number) {
                    continue;
                }
                let year: u16 = merged_at.get(..4).and_then(|s| s.parse().ok()).unwrap_or(0);
                if year >= cutoff_year {
                    out.push(RepoInfo {
                        name: repo.clone(),
                        stars: meta.stars,
                        year,
                        count: 1,
                    });
                }
            }
        }
    }

    Ok(out)
}
