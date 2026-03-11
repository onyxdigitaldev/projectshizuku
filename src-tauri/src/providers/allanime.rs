use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const ALLANIME_API: &str = "https://api.allanime.day";
const ALLANIME_REFR: &str = "https://allmanga.to";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:131.0) Gecko/20100101 Firefox/131.0";

/// Decode the obfuscated provider IDs from AllAnime
fn decode_provider_id(encoded: &str) -> String {
    let map: &[(&str, &str)] = &[
        ("79", "A"), ("7a", "B"), ("7b", "C"), ("7c", "D"), ("7d", "E"),
        ("7e", "F"), ("7f", "G"), ("70", "H"), ("71", "I"), ("72", "J"),
        ("73", "K"), ("74", "L"), ("75", "M"), ("76", "N"), ("77", "O"),
        ("68", "P"), ("69", "Q"), ("6a", "R"), ("6b", "S"), ("6c", "T"),
        ("6d", "U"), ("6e", "V"), ("6f", "W"), ("60", "X"), ("61", "Y"),
        ("62", "Z"), ("59", "a"), ("5a", "b"), ("5b", "c"), ("5c", "d"),
        ("5d", "e"), ("5e", "f"), ("5f", "g"), ("50", "h"), ("51", "i"),
        ("52", "j"), ("53", "k"), ("54", "l"), ("55", "m"), ("56", "n"),
        ("57", "o"), ("48", "p"), ("49", "q"), ("4a", "r"), ("4b", "s"),
        ("4c", "t"), ("4d", "u"), ("4e", "v"), ("4f", "w"), ("40", "x"),
        ("41", "y"), ("42", "z"), ("08", "0"), ("09", "1"), ("0a", "2"),
        ("0b", "3"), ("0c", "4"), ("0d", "5"), ("0e", "6"), ("0f", "7"),
        ("00", "8"), ("01", "9"), ("15", "-"), ("16", "."), ("67", "_"),
        ("46", "~"), ("02", ":"), ("17", "/"), ("07", "?"), ("1b", "#"),
        ("63", "["), ("65", "]"), ("78", "@"), ("19", "!"), ("1c", "$"),
        ("1e", "&"), ("10", "("), ("11", ")"), ("12", "*"), ("13", "+"),
        ("14", ","), ("03", ";"), ("05", "="), ("1d", "%"),
    ];

    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = encoded.chars().collect();

    while i < chars.len() {
        if i + 1 < chars.len() {
            let pair: String = chars[i..i + 2].iter().collect();
            if let Some((_, decoded)) = map.iter().find(|(k, _)| *k == pair.as_str()) {
                result.push_str(decoded);
                i += 2;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result.replace("/clock", "/clock.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeSource {
    pub quality: String,
    pub url: String,
    pub provider: String,
}

#[derive(Debug, Deserialize)]
struct EpisodeResponse {
    data: Option<EpisodeData>,
}

#[derive(Debug, Deserialize)]
struct EpisodeData {
    episode: Option<EpisodeNode>,
}

#[derive(Debug, Deserialize)]
struct EpisodeNode {
    #[serde(alias = "sourceUrls")]
    source_urls: Option<Vec<SourceUrl>>,
}

#[derive(Debug, Deserialize)]
struct SourceUrl {
    #[serde(alias = "sourceUrl")]
    source_url: String,
    #[serde(alias = "sourceName")]
    source_name: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    data: Option<SearchData>,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    shows: Option<ShowsData>,
}

#[derive(Debug, Deserialize)]
struct ShowsData {
    edges: Vec<ShowEdge>,
}

#[derive(Debug, Deserialize)]
struct ShowEdge {
    #[serde(alias = "_id")]
    id: String,
    name: String,
    #[serde(alias = "availableEpisodes")]
    available_episodes: Option<AvailableEpisodes>,
}

#[derive(Debug, Deserialize)]
struct AvailableEpisodes {
    sub: Option<i32>,
    dub: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct EpisodesListResponse {
    data: Option<ShowDetailData>,
}

#[derive(Debug, Deserialize)]
struct ShowDetailData {
    show: Option<ShowDetail>,
}

#[derive(Debug, Deserialize)]
struct ShowDetail {
    #[serde(alias = "availableEpisodesDetail")]
    available_episodes_detail: Option<EpisodesDetail>,
}

#[derive(Debug, Deserialize)]
struct EpisodesDetail {
    sub: Option<Vec<String>>,
    dub: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllAnimeShow {
    pub id: String,
    pub name: String,
    pub sub_count: Option<i32>,
    pub dub_count: Option<i32>,
}

pub struct AllAnimeClient {
    client: Client,
}

impl AllAnimeClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn search(&self, query: &str, mode: &str) -> Result<Vec<AllAnimeShow>> {
        let gql = r#"query($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType) { shows(search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin) { edges { _id name availableEpisodes __typename } } }"#;

        let variables = serde_json::json!({
            "search": {
                "allowAdult": false,
                "allowUnknown": false,
                "query": query
            },
            "limit": 40,
            "page": 1,
            "translationType": mode,
            "countryOrigin": "ALL"
        });

        let resp: SearchResponse = self
            .client
            .get(format!("{}/api", ALLANIME_API))
            .header("Referer", ALLANIME_REFR)
            .query(&[
                ("variables", serde_json::to_string(&variables)?),
                ("query", gql.to_string()),
            ])
            .send()
            .await?
            .json()
            .await?;

        let shows = resp
            .data
            .and_then(|d| d.shows)
            .map(|s| {
                s.edges
                    .into_iter()
                    .map(|e| AllAnimeShow {
                        id: e.id,
                        name: e.name,
                        sub_count: e.available_episodes.as_ref().and_then(|a| a.sub),
                        dub_count: e.available_episodes.as_ref().and_then(|a| a.dub),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(shows)
    }

    pub async fn get_episodes(&self, show_id: &str, mode: &str) -> Result<Vec<String>> {
        let gql = r#"query ($showId: String!) { show(_id: $showId) { _id availableEpisodesDetail } }"#;

        let variables = serde_json::json!({ "showId": show_id });

        let resp: EpisodesListResponse = self
            .client
            .get(format!("{}/api", ALLANIME_API))
            .header("Referer", ALLANIME_REFR)
            .query(&[
                ("variables", serde_json::to_string(&variables)?),
                ("query", gql.to_string()),
            ])
            .send()
            .await?
            .json()
            .await?;

        let mut episodes = resp
            .data
            .and_then(|d| d.show)
            .and_then(|s| s.available_episodes_detail)
            .and_then(|d| if mode == "dub" { d.dub } else { d.sub })
            .unwrap_or_default();

        // Sort numerically
        episodes.sort_by(|a, b| {
            a.parse::<f64>()
                .unwrap_or(0.0)
                .partial_cmp(&b.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(episodes)
    }

    pub async fn get_episode_sources(
        &self,
        show_id: &str,
        episode: &str,
        mode: &str,
    ) -> Result<Vec<EpisodeSource>> {
        let gql = r#"query ($showId: String!, $translationType: VaildTranslationTypeEnumType!, $episodeString: String!) { episode(showId: $showId translationType: $translationType episodeString: $episodeString) { episodeString sourceUrls } }"#;

        let variables = serde_json::json!({
            "showId": show_id,
            "translationType": mode,
            "episodeString": episode,
        });

        let resp: EpisodeResponse = self
            .client
            .get(format!("{}/api", ALLANIME_API))
            .header("Referer", ALLANIME_REFR)
            .query(&[
                ("variables", serde_json::to_string(&variables)?),
                ("query", gql.to_string()),
            ])
            .send()
            .await?
            .json()
            .await?;

        let source_urls = resp
            .data
            .and_then(|d| d.episode)
            .and_then(|e| e.source_urls)
            .unwrap_or_default();

        let mut sources = Vec::new();
        for su in source_urls {
            // Strip leading "--" from encoded URLs
            let encoded = su.source_url.strip_prefix("--").unwrap_or(&su.source_url);
            let decoded = decode_provider_id(encoded);

            // Fetch the actual video link from the decoded provider URL
            if let Ok(links) = self.fetch_provider_links(&decoded).await {
                for link in links {
                    sources.push(EpisodeSource {
                        quality: link.0,
                        url: link.1,
                        provider: su.source_name.clone(),
                    });
                }
            }
        }

        Ok(sources)
    }

    async fn fetch_provider_links(&self, provider_url: &str) -> Result<Vec<(String, String)>> {
        let url = format!("https://allanime.day{}", provider_url);
        let resp = self
            .client
            .get(&url)
            .header("Referer", ALLANIME_REFR)
            .send()
            .await?
            .text()
            .await?;

        let mut links = Vec::new();

        // Parse links from the JSON response
        // Format: "link":"<url>","resolutionStr":"<quality>"
        for part in resp.split("},{") {
            if let (Some(link_start), Some(res_start)) =
                (part.find("\"link\":\""), part.find("\"resolutionStr\":\""))
            {
                let link = &part[link_start + 8..];
                if let Some(link_end) = link.find('"') {
                    let link_val = &link[..link_end];
                    let res = &part[res_start + 18..];
                    if let Some(res_end) = res.find('"') {
                        let res_val = &res[..res_end];
                        links.push((res_val.to_string(), link_val.to_string()));
                    }
                }
            }
        }

        // Also check for HLS URLs
        if let Some(hls_start) = resp.find("\"hls\"") {
            if let Some(url_start) = resp[hls_start..].find("\"url\":\"") {
                let url_part = &resp[hls_start + url_start + 7..];
                if let Some(url_end) = url_part.find('"') {
                    links.push(("auto".to_string(), url_part[..url_end].to_string()));
                }
            }
        }

        Ok(links)
    }
}
