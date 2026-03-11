use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const ANILIST_URL: &str = "https://graphql.anilist.co";

// --- Public types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anime {
    pub id: i64,
    pub title_english: Option<String>,
    pub title_romaji: Option<String>,
    pub cover_image: Option<String>,
    pub banner_image: Option<String>,
    pub description: Option<String>,
    pub episodes: Option<i32>,
    pub status: Option<String>,
    pub format: Option<String>,
    pub genres: Vec<String>,
    pub average_score: Option<i32>,
    pub season: Option<String>,
    pub season_year: Option<i32>,
    pub next_airing_episode: Option<AiringEpisode>,
    pub relations: Vec<RelatedAnime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiringEpisode {
    pub episode: i32,
    pub airing_at: i64,
    pub time_until_airing: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedAnime {
    pub id: i64,
    pub relation_type: String,
    pub title_english: Option<String>,
    pub title_romaji: Option<String>,
    pub cover_image: Option<String>,
    pub episodes: Option<i32>,
    pub status: Option<String>,
    pub format: Option<String>,
    pub season_year: Option<i32>,
}

// --- Internal deserialization types ---

#[derive(Debug, Deserialize)]
struct GqlResponse<T> {
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct PageData {
    #[serde(alias = "Page")]
    page: PageInner,
}

#[derive(Debug, Deserialize)]
struct PageInner {
    media: Vec<MediaNode>,
}

#[derive(Debug, Deserialize)]
struct MediaNode {
    id: i64,
    title: Option<TitleNode>,
    #[serde(alias = "coverImage")]
    cover_image: Option<CoverNode>,
    #[serde(alias = "bannerImage")]
    banner_image: Option<String>,
    description: Option<String>,
    episodes: Option<i32>,
    status: Option<String>,
    format: Option<String>,
    genres: Option<Vec<String>>,
    #[serde(alias = "averageScore")]
    average_score: Option<i32>,
    season: Option<String>,
    #[serde(alias = "seasonYear")]
    season_year: Option<i32>,
    #[serde(alias = "nextAiringEpisode")]
    next_airing_episode: Option<AiringNode>,
    relations: Option<RelationsNode>,
}

#[derive(Debug, Deserialize)]
struct TitleNode {
    english: Option<String>,
    romaji: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CoverNode {
    #[serde(alias = "extraLarge")]
    extra_large: Option<String>,
    large: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AiringNode {
    episode: Option<i32>,
    #[serde(alias = "airingAt")]
    airing_at: Option<i64>,
    #[serde(alias = "timeUntilAiring")]
    time_until_airing: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RelationsNode {
    edges: Option<Vec<RelationEdge>>,
}

#[derive(Debug, Deserialize)]
struct RelationEdge {
    #[serde(alias = "relationType")]
    relation_type: Option<String>,
    node: Option<RelationMediaNode>,
}

#[derive(Debug, Deserialize)]
struct RelationMediaNode {
    id: i64,
    title: Option<TitleNode>,
    #[serde(alias = "coverImage")]
    cover_image: Option<CoverNode>,
    episodes: Option<i32>,
    status: Option<String>,
    format: Option<String>,
    #[serde(alias = "seasonYear")]
    season_year: Option<i32>,
}

impl From<MediaNode> for Anime {
    fn from(m: MediaNode) -> Self {
        let title = m.title.as_ref();
        let cover = m.cover_image.as_ref();
        let relations = m
            .relations
            .and_then(|r| r.edges)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|edge| {
                let node = edge.node?;
                let t = node.title.as_ref();
                let c = node.cover_image.as_ref();
                Some(RelatedAnime {
                    id: node.id,
                    relation_type: edge.relation_type.unwrap_or_default(),
                    title_english: t.and_then(|t| t.english.clone()),
                    title_romaji: t.and_then(|t| t.romaji.clone()),
                    cover_image: c.and_then(|c| c.extra_large.clone().or(c.large.clone())),
                    episodes: node.episodes,
                    status: node.status,
                    format: node.format,
                    season_year: node.season_year,
                })
            })
            .collect();

        Anime {
            id: m.id,
            title_english: title.and_then(|t| t.english.clone()),
            title_romaji: title.and_then(|t| t.romaji.clone()),
            cover_image: cover.and_then(|c| c.extra_large.clone().or(c.large.clone())),
            banner_image: m.banner_image,
            description: m.description,
            episodes: m.episodes,
            status: m.status,
            format: m.format,
            genres: m.genres.unwrap_or_default(),
            average_score: m.average_score,
            season: m.season,
            season_year: m.season_year,
            next_airing_episode: m.next_airing_episode.and_then(|a| {
                Some(AiringEpisode {
                    episode: a.episode?,
                    airing_at: a.airing_at?,
                    time_until_airing: a.time_until_airing?,
                })
            }),
            relations,
        }
    }
}

// --- Shared field fragments ---

const MEDIA_FIELDS: &str = r#"
    id
    title { english romaji }
    coverImage { extraLarge large }
    bannerImage
    description(asHtml: false)
    episodes
    status
    format
    genres
    averageScore
    season
    seasonYear
    nextAiringEpisode { episode airingAt timeUntilAiring }
"#;

const MEDIA_FIELDS_WITH_RELATIONS: &str = r#"
    id
    title { english romaji }
    coverImage { extraLarge large }
    bannerImage
    description(asHtml: false)
    episodes
    status
    format
    genres
    averageScore
    season
    seasonYear
    nextAiringEpisode { episode airingAt timeUntilAiring }
    relations {
        edges {
            relationType
            node {
                id
                title { english romaji }
                coverImage { extraLarge large }
                episodes
                status
                format
                seasonYear
            }
        }
    }
"#;

// --- Client ---

pub struct AniListClient {
    client: Client,
}

impl AniListClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    async fn query<T: for<'de> Deserialize<'de>>(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<T> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables,
        });
        let resp = self
            .client
            .post(ANILIST_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await?
            .json::<GqlResponse<T>>()
            .await?;
        resp.data
            .ok_or_else(|| anyhow::anyhow!("No data in AniList response"))
    }

    pub async fn search(&self, search: &str, page: i32, per_page: i32, allow_adult: bool) -> Result<Vec<Anime>> {
        let adult_filter = if allow_adult { "" } else { ", isAdult: false" };
        let q = format!(
            r#"query ($search: String, $page: Int, $perPage: Int) {{
                Page(page: $page, perPage: $perPage) {{
                    media(search: $search, type: ANIME, sort: POPULARITY_DESC{}) {{
                        {}
                    }}
                }}
            }}"#,
            adult_filter, MEDIA_FIELDS
        );
        let vars = serde_json::json!({
            "search": search,
            "page": page,
            "perPage": per_page,
        });
        let data: PageData = self.query(&q, vars).await?;
        Ok(data.page.media.into_iter().map(Anime::from).collect())
    }

    pub async fn trending(&self, page: i32, per_page: i32, allow_adult: bool) -> Result<Vec<Anime>> {
        let adult_filter = if allow_adult { "" } else { ", isAdult: false" };
        let q = format!(
            r#"query ($page: Int, $perPage: Int) {{
                Page(page: $page, perPage: $perPage) {{
                    media(type: ANIME, sort: TRENDING_DESC{}) {{
                        {}
                    }}
                }}
            }}"#,
            adult_filter, MEDIA_FIELDS
        );
        let vars = serde_json::json!({
            "page": page,
            "perPage": per_page,
        });
        let data: PageData = self.query(&q, vars).await?;
        Ok(data.page.media.into_iter().map(Anime::from).collect())
    }

    pub async fn recently_updated(&self, page: i32, per_page: i32, allow_adult: bool) -> Result<Vec<Anime>> {
        let adult_filter = if allow_adult { "" } else { ", isAdult: false" };
        let q = format!(
            r#"query ($page: Int, $perPage: Int) {{
                Page(page: $page, perPage: $perPage) {{
                    media(type: ANIME, status: RELEASING, sort: UPDATED_AT_DESC{}) {{
                        {}
                    }}
                }}
            }}"#,
            adult_filter, MEDIA_FIELDS
        );
        let vars = serde_json::json!({
            "page": page,
            "perPage": per_page,
        });
        let data: PageData = self.query(&q, vars).await?;
        Ok(data.page.media.into_iter().map(Anime::from).collect())
    }

    pub async fn get_anime(&self, id: i64) -> Result<Anime> {
        let q = format!(
            r#"query ($id: Int) {{
                Page(page: 1, perPage: 1) {{
                    media(id: $id, type: ANIME) {{
                        {}
                    }}
                }}
            }}"#,
            MEDIA_FIELDS_WITH_RELATIONS
        );
        let vars = serde_json::json!({ "id": id });
        let data: PageData = self.query(&q, vars).await?;
        data.page
            .media
            .into_iter()
            .next()
            .map(Anime::from)
            .ok_or_else(|| anyhow::anyhow!("Anime not found"))
    }

    pub async fn browse_by_genre(&self, genre: &str, page: i32, per_page: i32, allow_adult: bool) -> Result<Vec<Anime>> {
        let adult_filter = if allow_adult { "" } else { ", isAdult: false" };
        let q = format!(
            r#"query ($page: Int, $perPage: Int, $genre: String) {{
                Page(page: $page, perPage: $perPage) {{
                    media(type: ANIME, genre: $genre, sort: POPULARITY_DESC{}) {{
                        {}
                    }}
                }}
            }}"#,
            adult_filter, MEDIA_FIELDS
        );
        let vars = serde_json::json!({
            "page": page,
            "perPage": per_page,
            "genre": genre,
        });
        let data: PageData = self.query(&q, vars).await?;
        Ok(data.page.media.into_iter().map(Anime::from).collect())
    }
}
