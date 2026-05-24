use regex::Regex;
use synapse_core::{ProfileId, ProfileMatch};
use tracing::instrument;

use crate::parser::LoadedProfile;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForegroundWindow {
    pub exe: Option<String>,
    pub title: Option<String>,
    pub steam_appid: Option<u32>,
    pub window_class: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MatchRank {
    WindowClass = 1,
    SteamAppId = 2,
    TitleRegex = 3,
    Exe = 4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileMatchResolution {
    pub profile_id: ProfileId,
    pub rank_name: &'static str,
}

#[instrument(skip_all, fields(profile_count = profiles.len()))]
#[must_use]
pub fn resolve_active_profile(
    profiles: &[LoadedProfile],
    foreground: &ForegroundWindow,
) -> Option<ProfileMatchResolution> {
    profiles
        .iter()
        .enumerate()
        .filter_map(|(index, loaded)| {
            best_rank(&loaded.profile.matches, foreground).map(|rank| (loaded, rank, index))
        })
        .max_by(
            |(left, left_rank, left_index), (right, right_rank, right_index)| {
                left_rank
                    .cmp(right_rank)
                    .then_with(|| left.modified.cmp(&right.modified))
                    .then_with(|| right.source_path.cmp(&left.source_path))
                    .then_with(|| right.profile.id.cmp(&left.profile.id))
                    .then_with(|| right_index.cmp(left_index))
            },
        )
        .map(|(loaded, rank, _index)| ProfileMatchResolution {
            profile_id: loaded.profile.id.clone(),
            rank_name: rank.name(),
        })
}

fn best_rank(matches: &[ProfileMatch], foreground: &ForegroundWindow) -> Option<MatchRank> {
    matches
        .iter()
        .filter_map(|candidate| candidate_rank(candidate, foreground))
        .max()
}

fn candidate_rank(candidate: &ProfileMatch, foreground: &ForegroundWindow) -> Option<MatchRank> {
    if candidate
        .exe
        .as_deref()
        .zip(foreground.exe.as_deref())
        .is_some_and(|(expected, actual)| expected.eq_ignore_ascii_case(actual))
    {
        return Some(MatchRank::Exe);
    }

    if candidate
        .title_regex
        .as_deref()
        .zip(foreground.title.as_deref())
        .is_some_and(|(pattern, title)| {
            Regex::new(pattern).is_ok_and(|regex| regex.is_match(title))
        })
    {
        return Some(MatchRank::TitleRegex);
    }

    if candidate
        .steam_appid
        .zip(foreground.steam_appid)
        .is_some_and(|(expected, actual)| expected == actual)
    {
        return Some(MatchRank::SteamAppId);
    }

    if candidate
        .window_class
        .as_deref()
        .zip(foreground.window_class.as_deref())
        .is_some_and(|(expected, actual)| expected.eq_ignore_ascii_case(actual))
    {
        return Some(MatchRank::WindowClass);
    }

    None
}

impl MatchRank {
    const fn name(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::TitleRegex => "title_regex",
            Self::SteamAppId => "steam_appid",
            Self::WindowClass => "window_class",
        }
    }
}
