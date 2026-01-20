use reqwest::header::{AUTHORIZATION, USER_AGENT};
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

// --- Public output type ---

#[derive(Clone)]
pub struct RepoInfo {
    pub name: String,
    pub stars: u32,
    pub year: u16,
    pub count: u32,
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
