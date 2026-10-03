use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::Deserialize;

use super::tmdb::{validate_id, validate_id_language, TmdbClient, TmdbError, TmdbImagesResponse};

const EPISODE_GROUP_ID_MAX_LEN: usize = 64;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TmdbEpisodeGroupsResponse {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub results: Vec<TmdbEpisodeGroupSummary>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TmdbEpisodeGroupSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub episode_count: i32,
    #[serde(default)]
    pub group_count: i32,
    #[serde(default, rename = "type")]
    pub group_type: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TmdbEpisodeGroupDetail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub groups: Vec<TmdbEpisodeGroupSection>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TmdbEpisodeGroupSection {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub episodes: Vec<TmdbEpisodeGroupEntry>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TmdbEpisodeGroupEntry {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub air_date: Option<String>,
    #[serde(default)]
    pub season_number: Option<i32>,
    #[serde(default)]
    pub episode_number: Option<i32>,
}

/// Ordered index over one TMDb episode group. Sections are keyed by the
/// section `order` field, which mirrors the library's local season numbers.
pub struct GroupIndex {
    pub group_id: String,
    pub group_name: Option<String>,
    sections: BTreeMap<i32, Vec<EpisodeSlot>>,
}

#[derive(Clone, Debug)]
pub struct EpisodeSlot {
    pub season_number: i32,
    pub episode_number: i32,
    pub episode_id: Option<i64>,
    pub name: Option<String>,
    pub air_date: Option<String>,
}

impl GroupIndex {
    pub fn section(&self, order: i32) -> Option<&Vec<EpisodeSlot>> {
        self.sections.get(&order)
    }

    /// Resolves a local (season, episode) pair to the real TMDb episode.
    /// `episode` is a 1-based position within the section.
    pub fn slot(&self, order: i32, episode: i32) -> Option<&EpisodeSlot> {
        if episode < 1 {
            return None;
        }
        self.section(order)
            .and_then(|slots| slots.get((episode - 1) as usize))
    }
}

pub fn build_group_index(detail: &TmdbEpisodeGroupDetail) -> GroupIndex {
    let mut sections: BTreeMap<i32, Vec<EpisodeSlot>> = BTreeMap::new();
    for (section_index, section) in detail.groups.iter().enumerate() {
        let order = section.order.unwrap_or(section_index as i32);
        let slots = section
            .episodes
            .iter()
            .enumerate()
            .map(|(position, entry)| EpisodeSlot {
                season_number: entry.season_number.unwrap_or(order),
                episode_number: entry.episode_number.unwrap_or(position as i32 + 1),
                episode_id: entry.id,
                name: entry.name.clone(),
                air_date: entry.air_date.clone(),
            })
            .collect::<Vec<_>>();
        sections.entry(order).or_default().extend(slots);
    }
    GroupIndex {
        group_id: detail.id.clone(),
        group_name: detail.name.clone(),
        sections,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeGroupStrategy {
    Aired,
    Absolute,
    Dvd,
}

impl EpisodeGroupStrategy {
    pub fn from_config(value: Option<&str>) -> Self {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            Some("aired") => Self::Aired,
            Some("dvd") => Self::Dvd,
            _ => Self::Absolute,
        }
    }

    fn type_id(self) -> i32 {
        match self {
            // TMDb episode group types: 1 aired/TVDB order, 2 absolute order,
            // 3 DVD order, 4 streaming release, 5 regional, 6 production.
            Self::Aired => 1,
            Self::Absolute => 2,
            Self::Dvd => 3,
        }
    }
}

/// Picks the group matching the strategy, preferring complete single-section
/// groups (for example "Absolute (No Specials)") and then the largest one.
pub fn pick_group<'a>(
    groups: &'a [TmdbEpisodeGroupSummary],
    strategy: EpisodeGroupStrategy,
) -> Option<&'a TmdbEpisodeGroupSummary> {
    let wanted = strategy.type_id();
    groups
        .iter()
        .filter(|group| group.group_type == Some(wanted))
        .max_by(|left, right| {
            let left_single = left.group_count == 1;
            let right_single = right.group_count == 1;
            left_single
                .cmp(&right_single)
                .then(left.episode_count.cmp(&right.episode_count))
                .then_with(|| match left.name.cmp(&right.name) {
                    Ordering::Greater => Ordering::Greater,
                    Ordering::Equal => Ordering::Equal,
                    Ordering::Less => Ordering::Less,
                })
        })
}

pub fn is_valid_group_id(group_id: &str) -> bool {
    !group_id.is_empty()
        && group_id.len() <= EPISODE_GROUP_ID_MAX_LEN
        && group_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
}

impl TmdbClient {
    pub async fn episode_groups_list(
        &self,
        series_id: i64,
        language: &str,
    ) -> Result<TmdbEpisodeGroupsResponse, TmdbError> {
        validate_id_language(series_id, language, "series")?;
        let endpoint = format!("3/tv/{series_id}/episode_groups");
        let params = [("language", language.trim().to_owned())];
        let response: TmdbEpisodeGroupsResponse = self.request_json(&endpoint, &params).await?;
        for group in &response.results {
            if !is_valid_group_id(&group.id) {
                return Err(TmdbError::InvalidResponse(
                    "TMDb episode group ID is invalid".to_owned(),
                ));
            }
        }
        Ok(response)
    }

    pub async fn episode_group_detail(
        &self,
        group_id: &str,
    ) -> Result<TmdbEpisodeGroupDetail, TmdbError> {
        if !is_valid_group_id(group_id) {
            return Err(TmdbError::InvalidRequest(
                "TMDb episode group ID is invalid".to_owned(),
            ));
        }
        let endpoint = format!("3/tv/episode_group/{group_id}");
        let detail: TmdbEpisodeGroupDetail = self
            .request_json(&endpoint, &[] as &[(String, String)])
            .await?;
        if detail.id != group_id {
            return Err(TmdbError::InvalidResponse(
                "TMDb episode group detail ID does not match".to_owned(),
            ));
        }
        Ok(detail)
    }

    /// Fetches images with an explicit `include_image_language` list.
    /// `None` keeps the default `{language},en,null` behaviour; the empty
    /// string language preserves the manual all-languages search semantics.
    pub async fn images_with_include_language(
        &self,
        item_type: &str,
        item_id: i64,
        language: Option<&str>,
        include_language: Option<String>,
    ) -> Result<TmdbImagesResponse, TmdbError> {
        validate_id(item_id, item_type)?;
        let endpoint = format!("3/{item_type}/{item_id}/images");
        images_request(self, endpoint, language, include_language).await
    }

    pub async fn season_images_with_include_language(
        &self,
        series_id: i64,
        season_number: i32,
        language: Option<&str>,
        include_language: Option<String>,
    ) -> Result<TmdbImagesResponse, TmdbError> {
        validate_id(series_id, "series")?;
        if !(-1..=1000).contains(&season_number) {
            return Err(TmdbError::InvalidRequest(
                "season number is out of range".to_owned(),
            ));
        }
        let endpoint = format!("3/tv/{series_id}/season/{season_number}/images");
        images_request(self, endpoint, language, include_language).await
    }

    pub async fn episode_images_with_include_language(
        &self,
        series_id: i64,
        season_number: i32,
        episode_number: i32,
        language: Option<&str>,
        include_language: Option<String>,
    ) -> Result<TmdbImagesResponse, TmdbError> {
        validate_id(series_id, "series")?;
        if !(-1..=1000).contains(&season_number) || !(0..=10000).contains(&episode_number) {
            return Err(TmdbError::InvalidRequest(
                "episode number is out of range".to_owned(),
            ));
        }
        let endpoint =
            format!("3/tv/{series_id}/season/{season_number}/episode/{episode_number}/images");
        images_request(self, endpoint, language, include_language).await
    }
}

async fn images_request(
    client: &TmdbClient,
    endpoint: String,
    language: Option<&str>,
    include_language: Option<String>,
) -> Result<TmdbImagesResponse, TmdbError> {
    if let Some(language) = language {
        if language.trim().is_empty() {
            return Err(TmdbError::InvalidRequest(
                "image language is invalid".to_owned(),
            ));
        }
    }
    if let Some(include) = include_language.as_deref() {
        validate_include_language(include)?;
    }
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(language) = language {
        params.push(("language", language.trim().to_owned()));
    }
    if let Some(include) = include_language {
        params.push(("include_image_language", include));
    }
    client.request_json(&endpoint, &params).await
}

fn validate_include_language(include: &str) -> Result<(), TmdbError> {
    if include.len() > 128
        || !include
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == ',' || character == '-')
    {
        return Err(TmdbError::InvalidRequest(
            "include_image_language is invalid".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        season_number: i32,
        episode_number: i32,
        id: i64,
        name: &str,
        air_date: &str,
    ) -> TmdbEpisodeGroupEntry {
        TmdbEpisodeGroupEntry {
            id: Some(id),
            name: Some(name.to_owned()),
            air_date: Some(air_date.to_owned()),
            season_number: Some(season_number),
            episode_number: Some(episode_number),
        }
    }

    #[test]
    fn group_index_maps_local_order_to_real_episodes() {
        let detail = TmdbEpisodeGroupDetail {
            id: "5ad0f096c3a36825a300e78b".to_owned(),
            name: Some("Absolute".to_owned()),
            groups: vec![
                TmdbEpisodeGroupSection {
                    order: Some(0),
                    name: Some("Specials".to_owned()),
                    episodes: vec![entry(0, 1, 10, "SP1", "2020-01-01")],
                    ..TmdbEpisodeGroupSection::default()
                },
                TmdbEpisodeGroupSection {
                    order: Some(1),
                    name: Some("Show".to_owned()),
                    episodes: vec![
                        entry(1, 1, 21, "E1", "2020-01-02"),
                        entry(2, 1, 22, "E2", "2020-01-03"),
                    ],
                    ..TmdbEpisodeGroupSection::default()
                },
            ],
        };

        let index = build_group_index(&detail);

        assert_eq!(index.section(1).map(|slots| slots.len()), Some(2));
        let slot = index.slot(1, 2).expect("second absolute episode");
        assert_eq!((slot.season_number, slot.episode_number), (2, 1));
        assert_eq!(slot.episode_id, Some(22));
        assert!(index.slot(1, 0).is_none());
        assert!(index.slot(9, 1).is_none());
    }

    #[test]
    fn pick_group_prefers_matching_type_then_single_section_then_largest() {
        let groups = vec![
            TmdbEpisodeGroupSummary {
                id: "aired".to_owned(),
                episode_count: 100,
                group_count: 9,
                group_type: Some(1),
                ..TmdbEpisodeGroupSummary::default()
            },
            TmdbEpisodeGroupSummary {
                id: "absolute-with-specials".to_owned(),
                episode_count: 1220,
                group_count: 2,
                group_type: Some(2),
                ..TmdbEpisodeGroupSummary::default()
            },
            TmdbEpisodeGroupSummary {
                id: "absolute-no-specials".to_owned(),
                episode_count: 1181,
                group_count: 1,
                group_type: Some(2),
                ..TmdbEpisodeGroupSummary::default()
            },
            TmdbEpisodeGroupSummary {
                id: "dvd".to_owned(),
                episode_count: 1047,
                group_count: 36,
                group_type: Some(3),
                ..TmdbEpisodeGroupSummary::default()
            },
        ];

        let absolute = pick_group(&groups, EpisodeGroupStrategy::Absolute).expect("absolute group");
        assert_eq!(absolute.id, "absolute-no-specials");
        let dvd = pick_group(&groups, EpisodeGroupStrategy::Dvd).expect("dvd group");
        assert_eq!(dvd.id, "dvd");
    }

    #[test]
    fn strategy_defaults_to_absolute_for_unknown_values() {
        assert_eq!(
            EpisodeGroupStrategy::from_config(Some("dvd")),
            EpisodeGroupStrategy::Dvd
        );
        assert_eq!(
            EpisodeGroupStrategy::from_config(Some("nonsense")),
            EpisodeGroupStrategy::Absolute
        );
        assert_eq!(
            EpisodeGroupStrategy::from_config(None),
            EpisodeGroupStrategy::Absolute
        );
    }
}
