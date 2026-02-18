//! ZSR Restreaming Export Module
//!
//! This module handles exporting race data to ZSR restreaming backend Google Sheets.
//! It supports multiple backends (ZSR, ZSRDE, ZSRFR) with configurable triggers.

use {
    chrono::Utc,
    chrono_tz::US::Eastern,
    chrono_tz::Europe::Berlin,
    crate::{
        cal::{Race, RaceSchedule, Entrant, Entrants},
        event::{self, roles::{RoleBinding, Signup, VolunteerSignupStatus}},
        id::Races,
        prelude::*,
        series::Series,
        sheets::{self, WriteError},
    },
};

// ============================================================================
// Error Types
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] Event(#[from] event::DataError),
    #[error(transparent)] Sheets(#[from] WriteError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error("backend not found: {0}")]
    BackendNotFound(i32),
    #[error("export config not found: {0}")]
    ExportNotFound(i32),
    #[error("race is not a live race")]
    NotLive,
    #[error("event not found")]
    EventNotFound,
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Cal(e) => e.is_network_error(),
            Self::Event(_) => false,
            Self::Sheets(e) => e.is_network_error(),
            _ => false,
        }
    }
}

// ============================================================================
// Domain Types
// ============================================================================

/// Trigger condition for when an export should fire
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "zsr_export_trigger", rename_all = "snake_case")]
pub(crate) enum ExportTrigger {
    WhenScheduled,
    WhenRestreamChannelSet,
    WhenVolunteerSignedUp,
}

impl fmt::Display for ExportTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WhenScheduled => write!(f, "When Scheduled"),
            Self::WhenRestreamChannelSet => write!(f, "When Restream Channel Set"),
            Self::WhenVolunteerSignedUp => write!(f, "When Volunteer Signed Up"),
        }
    }
}

/// A ZSR restreaming backend configuration
#[derive(Debug, Clone)]
pub(crate) struct RestreamingBackend {
    pub(crate) id: i32,
    pub(crate) name: String,
    pub(crate) google_sheet_id: String,
    pub(crate) language: Language,
    pub(crate) hth_export_id_col: String,
    pub(crate) commentators_col: String,
    pub(crate) trackers_col: String,
    pub(crate) restream_channel_col: Option<String>,
    pub(crate) notes_col: String,
    pub(crate) dst_formula_standard: String,
    pub(crate) dst_formula_dst: String,
}

impl RestreamingBackend {
    /// Load a backend by ID
    pub(crate) async fn from_id(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                id,
                name,
                google_sheet_id,
                language AS "language: Language",
                hth_export_id_col,
                commentators_col,
                trackers_col,
                restream_channel_col,
                notes_col,
                dst_formula_standard,
                dst_formula_dst
            FROM zsr_restreaming_backends
            WHERE id = $1
        "#, id)
        .fetch_optional(&mut **transaction)
        .await
    }

    /// Load all backends
    pub(crate) async fn all(
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                id,
                name,
                google_sheet_id,
                language AS "language: Language",
                hth_export_id_col,
                commentators_col,
                trackers_col,
                restream_channel_col,
                notes_col,
                dst_formula_standard,
                dst_formula_dst
            FROM zsr_restreaming_backends
            ORDER BY name
        "#)
        .fetch_all(&mut **transaction)
        .await
    }

    /// Create a new backend
    pub(crate) async fn create(
        transaction: &mut Transaction<'_, Postgres>,
        name: &str,
        google_sheet_id: &str,
        language: Language,
        hth_export_id_col: &str,
        commentators_col: &str,
        trackers_col: &str,
        restream_channel_col: Option<&str>,
        notes_col: &str,
        dst_formula_standard: &str,
        dst_formula_dst: &str,
    ) -> Result<i32, sqlx::Error> {
        let row = sqlx::query_scalar!(r#"
            INSERT INTO zsr_restreaming_backends (
                name, google_sheet_id, language,
                hth_export_id_col, commentators_col, trackers_col,
                restream_channel_col, notes_col,
                dst_formula_standard, dst_formula_dst
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id
        "#,
            name,
            google_sheet_id,
            language as _,
            hth_export_id_col,
            commentators_col,
            trackers_col,
            restream_channel_col as _,
            notes_col,
            dst_formula_standard,
            dst_formula_dst
        )
        .fetch_one(&mut **transaction)
        .await?;
        Ok(row)
    }

    /// Update a backend
    pub(crate) async fn update(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
        name: &str,
        google_sheet_id: &str,
        language: Language,
        hth_export_id_col: &str,
        commentators_col: &str,
        trackers_col: &str,
        restream_channel_col: Option<&str>,
        notes_col: &str,
        dst_formula_standard: &str,
        dst_formula_dst: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(r#"
            UPDATE zsr_restreaming_backends SET
                name = $2,
                google_sheet_id = $3,
                language = $4,
                hth_export_id_col = $5,
                commentators_col = $6,
                trackers_col = $7,
                restream_channel_col = $8,
                notes_col = $9,
                dst_formula_standard = $10,
                dst_formula_dst = $11,
                updated_at = NOW()
            WHERE id = $1
        "#,
            id,
            name,
            google_sheet_id,
            language as _,
            hth_export_id_col,
            commentators_col,
            trackers_col,
            restream_channel_col as _,
            notes_col,
            dst_formula_standard,
            dst_formula_dst
        )
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    /// Delete a backend
    pub(crate) async fn delete(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM zsr_restreaming_backends WHERE id = $1", id)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }
}

/// An export configuration for a specific event to a specific backend
#[derive(Debug, Clone)]
pub(crate) struct ExportConfig {
    pub(crate) id: i32,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) backend_id: i32,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) estimate_override: Option<String>,
    pub(crate) delay_minutes: i32,
    pub(crate) nodecg_pk: Option<i32>,
    pub(crate) trigger_condition: ExportTrigger,
    pub(crate) enabled: bool,
}

impl ExportConfig {
    /// Load an export config by ID
    pub(crate) async fn from_id(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                id,
                series AS "series: Series",
                event,
                backend_id,
                title,
                description,
                estimate_override,
                delay_minutes,
                nodecg_pk,
                trigger_condition AS "trigger_condition: ExportTrigger",
                enabled
            FROM zsr_restream_exports
            WHERE id = $1
        "#, id)
        .fetch_optional(&mut **transaction)
        .await
    }

    /// Load all export configs for an event
    pub(crate) async fn for_event(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                id,
                series AS "series: Series",
                event,
                backend_id,
                title,
                description,
                estimate_override,
                delay_minutes,
                nodecg_pk,
                trigger_condition AS "trigger_condition: ExportTrigger",
                enabled
            FROM zsr_restream_exports
            WHERE series = $1 AND event = $2
            ORDER BY backend_id
        "#, series as _, event)
        .fetch_all(&mut **transaction)
        .await
    }

    /// Load all enabled export configs
    pub(crate) async fn all_enabled(
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                id,
                series AS "series: Series",
                event,
                backend_id,
                title,
                description,
                estimate_override,
                delay_minutes,
                nodecg_pk,
                trigger_condition AS "trigger_condition: ExportTrigger",
                enabled
            FROM zsr_restream_exports
            WHERE enabled = true
            ORDER BY series, event, backend_id
        "#)
        .fetch_all(&mut **transaction)
        .await
    }

    /// Create a new export config
    pub(crate) async fn create(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        backend_id: i32,
        title: Option<&str>,
        description: Option<&str>,
        estimate_override: Option<&str>,
        delay_minutes: i32,
        nodecg_pk: Option<i32>,
        trigger_condition: ExportTrigger,
    ) -> Result<Self, sqlx::Error> {
        let id = sqlx::query_scalar!(r#"
            INSERT INTO zsr_restream_exports (
                series, event, backend_id,
                title, description, estimate_override, delay_minutes, nodecg_pk,
                trigger_condition
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id
        "#,
            series as _,
            event,
            backend_id,
            title,
            description,
            estimate_override,
            delay_minutes,
            nodecg_pk,
            trigger_condition as _
        )
        .fetch_one(&mut **transaction)
        .await?;

        Ok(Self {
            id,
            series,
            event: event.to_owned(),
            backend_id,
            title: title.map(|s| s.to_owned()),
            description: description.map(|s| s.to_owned()),
            estimate_override: estimate_override.map(|s| s.to_owned()),
            delay_minutes,
            nodecg_pk,
            trigger_condition,
            enabled: true,
        })
    }

    /// Update an export config
    pub(crate) async fn update(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
        title: Option<&str>,
        description: Option<&str>,
        estimate_override: Option<&str>,
        delay_minutes: i32,
        nodecg_pk: Option<i32>,
        trigger_condition: ExportTrigger,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(r#"
            UPDATE zsr_restream_exports SET
                title = $2,
                description = $3,
                estimate_override = $4,
                delay_minutes = $5,
                nodecg_pk = $6,
                trigger_condition = $7,
                enabled = $8,
                updated_at = NOW()
            WHERE id = $1
        "#,
            id,
            title,
            description,
            estimate_override,
            delay_minutes,
            nodecg_pk,
            trigger_condition as _,
            enabled
        )
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    /// Delete an export config
    pub(crate) async fn delete(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM zsr_restream_exports WHERE id = $1", id)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    /// Get the associated backend
    #[allow(dead_code)]
    pub(crate) async fn backend(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<Option<RestreamingBackend>, sqlx::Error> {
        RestreamingBackend::from_id(transaction, self.backend_id).await
    }
}

/// Tracks an exported race to a backend
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct RaceExport {
    pub(crate) race_id: Id<Races>,
    pub(crate) export_id: i32,
    pub(crate) sheet_row_id: String,
    pub(crate) exported_at: DateTime<Utc>,
    pub(crate) last_synced_at: DateTime<Utc>,
}

impl RaceExport {
    /// Check if a race has been exported to a specific export config
    pub(crate) async fn find(
        transaction: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
        export_id: i32,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                race_id AS "race_id: Id<Races>",
                export_id,
                sheet_row_id,
                exported_at,
                last_synced_at
            FROM zsr_race_exports
            WHERE race_id = $1 AND export_id = $2
        "#, race_id as _, export_id)
        .fetch_optional(&mut **transaction)
        .await
    }

    /// Get all exports for a race
    #[allow(dead_code)]
    pub(crate) async fn for_race(
        transaction: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(Self, r#"
            SELECT
                race_id AS "race_id: Id<Races>",
                export_id,
                sheet_row_id,
                exported_at,
                last_synced_at
            FROM zsr_race_exports
            WHERE race_id = $1
        "#, race_id as _)
        .fetch_all(&mut **transaction)
        .await
    }

    /// Insert or update a race export record
    pub(crate) async fn upsert(
        transaction: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
        export_id: i32,
        sheet_row_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(r#"
            INSERT INTO zsr_race_exports (race_id, export_id, sheet_row_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (race_id, export_id)
            DO UPDATE SET
                sheet_row_id = $3,
                last_synced_at = NOW()
        "#, race_id as _, export_id, sheet_row_id)
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    /// Delete a race export record
    pub(crate) async fn delete(
        transaction: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
        export_id: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(r#"
            DELETE FROM zsr_race_exports
            WHERE race_id = $1 AND export_id = $2
        "#, race_id as _, export_id)
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }
}

// ============================================================================
// Export Logic
// ============================================================================

/// Check if a race should be exported based on the trigger condition
pub(crate) async fn should_export_race(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    export: &ExportConfig,
    backend: &RestreamingBackend,
) -> Result<bool, Error> {
    // Only export live races (not asyncs)
    let RaceSchedule::Live { .. } = race.schedule else {
        return Ok(false);
    };

    // Race must not be ignored
    if race.ignored {
        return Ok(false);
    }

    // Check restream consent for all teams
    if let Some(mut teams) = race.teams_opt() {
        if !teams.all(|team| team.restream_consent) {
            return Ok(false);
        }
    }

    // Check trigger condition
    match export.trigger_condition {
        ExportTrigger::WhenScheduled => {
            // Export if race has a scheduled start time
            Ok(true)
        }
        ExportTrigger::WhenRestreamChannelSet => {
            // Export if video URL for this backend's language is set
            Ok(race.video_urls.contains_key(&backend.language))
        }
        ExportTrigger::WhenVolunteerSignedUp => {
            // Export if at least one volunteer has signed up (pending or confirmed)
            let signups = Signup::for_race(transaction, race.id).await?;
            Ok(signups.iter().any(|s| matches!(
                s.status,
                VolunteerSignupStatus::Pending | VolunteerSignupStatus::Confirmed
            )))
        }
    }
}

/// Check if a race should be removed from the sheet
pub(crate) fn should_remove_race(race: &Race) -> bool {
    // Remove if race is ignored
    if race.ignored {
        return true;
    }

    // Remove if schedule is reset (no start time)
    match race.schedule {
        RaceSchedule::Unscheduled => true,
        RaceSchedule::Async { .. } => true, // Remove if changed to async
        RaceSchedule::Live { .. } => false,
    }
}

/// Generate the unique export ID for a race
pub(crate) fn generate_export_id(race: &Race, export: &ExportConfig) -> String {
    format!("HTH-{}-{}-{}", export.series.slug(), export.event, race.id)
}

/// Check if a datetime is during US Eastern DST
pub(crate) fn is_us_eastern_dst(dt: DateTime<Utc>) -> bool {
    let eastern = dt.with_timezone(&Eastern);
    // During DST (EDT), UTC offset is -4 hours; during standard time (EST), it's -5 hours
    eastern.offset().fix().local_minus_utc() == -4 * 3600
}

/// Check if a datetime is during German DST (CEST)
pub(crate) fn is_german_dst(dt: DateTime<Utc>) -> bool {
    let berlin = dt.with_timezone(&Berlin);
    // During DST (CEST), UTC offset is +2 hours; during standard time (CET), it's +1 hour
    berlin.offset().fix().local_minus_utc() == 2 * 3600
}

/// Build the title string for a race
pub(crate) async fn build_race_title(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    export: &ExportConfig,
    event_display_name: &str,
) -> String {
    // Use export title if set, otherwise event display name
    let event_name = export.title.as_deref().unwrap_or(event_display_name);

    // Build matchup string
    let matchup = match &race.entrants {
        Entrants::Two([e1, e2]) => {
            let name1 = get_entrant_name(transaction, e1).await.unwrap_or_else(|| "TBD".to_owned());
            let name2 = get_entrant_name(transaction, e2).await.unwrap_or_else(|| "TBD".to_owned());
            format!("{} vs. {}", name1, name2)
        }
        Entrants::Three([e1, e2, e3]) => {
            let name1 = get_entrant_name(transaction, e1).await.unwrap_or_else(|| "TBD".to_owned());
            let name2 = get_entrant_name(transaction, e2).await.unwrap_or_else(|| "TBD".to_owned());
            let name3 = get_entrant_name(transaction, e3).await.unwrap_or_else(|| "TBD".to_owned());
            format!("{} vs. {} vs. {}", name1, name2, name3)
        }
        _ => "TBD".to_owned(),
    };

    // Combine: EventName: Round - Matchup
    if let Some(round) = &race.round {
        format!("{}: {} - {}", event_name, round, matchup)
    } else if let Some(phase) = &race.phase {
        format!("{}: {} - {}", event_name, phase, matchup)
    } else {
        format!("{}: {}", event_name, matchup)
    }
}

/// Get display name for an entrant
async fn get_entrant_name(transaction: &mut Transaction<'_, Postgres>, entrant: &Entrant) -> Option<String> {
    match entrant {
        Entrant::MidosHouseTeam(team) => {
            team.name(transaction).await.ok().flatten().map(|n| n.into_owned())
        }
        Entrant::Named { name, .. } => Some(name.clone()),
        Entrant::Discord { .. } => Some("Discord User".to_owned()),
    }
}

/// Get the number of runners/players in a race
pub(crate) fn get_runner_count(race: &Race) -> i32 {
    match &race.entrants {
        Entrants::Two(_) => 2,
        Entrants::Three(_) => 3,
        Entrants::Count { total, .. } => *total as i32,
        _ => 2, // Default to 2 for solo races
    }
}

/// Format a TimeDelta as HH:MM:SS for the estimate column
pub(crate) fn format_estimate(duration: TimeDelta) -> String {
    let total_seconds = duration.num_seconds();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Ensure the event description entry exists in the Descriptions sheet on the backend.
/// Columns: Name, Description, Estimate, Runners, PK (starting at row 2)
pub(crate) async fn ensure_description_entry(
    http_client: &reqwest::Client,
    export: &ExportConfig,
    backend: &RestreamingBackend,
    event_display_name: &str,
    default_runner_count: i32,
) -> Result<(), Error> {
    // Get the title to use (export title override or event display name)
    let title = export.title.as_deref().unwrap_or(event_display_name);

    // Read existing descriptions from the sheet
    let existing = sheets::read_values_uncached(
        http_client,
        &backend.google_sheet_id,
        "'Descriptions'!A:A",
    ).await?;

    // Check if title already exists (starting from row 2, so index 1)
    let title_exists = existing.iter()
        .skip(1) // Skip header row
        .any(|row| row.first().map(|s| s == title).unwrap_or(false));

    if !title_exists {
        // Get the estimate - use override if present, otherwise calculate from series default duration
        let estimate = export.estimate_override.clone()
            .unwrap_or_else(|| format_estimate(export.series.default_race_duration()));

        // Build the row data
        let values = vec![vec![
            title.to_owned(),                                    // Name
            export.description.clone().unwrap_or_default(),      // Description
            estimate,                                            // Estimate
            default_runner_count.to_string(),                    // Runners
            export.nodecg_pk.map(|pk| pk.to_string()).unwrap_or_default(), // PK
        ]];

        // Append to the Descriptions sheet
        sheets::append_values(
            http_client,
            &backend.google_sheet_id,
            "'Descriptions'!A:E",
            values,
        ).await?;
    }

    Ok(())
}

/// Get confirmed volunteers for a specific role type
pub(crate) async fn get_volunteers_for_role(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    role_type_name: &str,
) -> Result<Vec<String>, Error> {
    let signups = Signup::for_race(transaction, race.id).await?;
    let role_bindings = RoleBinding::for_event(transaction, race.series, &race.event).await?;
    let mut names = Vec::new();

    for signup in signups {
        if matches!(signup.status, VolunteerSignupStatus::Confirmed) {
            // Find the role binding for this signup
            if let Some(binding) = role_bindings.iter().find(|b| b.id == signup.role_binding_id) {
                if binding.role_type_name.to_lowercase().contains(&role_type_name.to_lowercase()) {
                    if let Ok(Some(user)) = User::from_id(&mut **transaction, signup.user_id).await {
                        names.push(user.display_name().to_owned());
                    }
                }
            }
        }
    }

    Ok(names)
}

/// Export a single race to a Google Sheet
pub(crate) async fn export_race(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    race: &Race,
    export: &ExportConfig,
    backend: &RestreamingBackend,
    event_display_name: &str,
    is_update: bool,
) -> Result<String, Error> {
    let RaceSchedule::Live { start, .. } = race.schedule else {
        return Err(Error::NotLive);
    };

    // Generate export ID
    let export_id = generate_export_id(race, export);

    // Calculate start time with delay
    let delayed_start = start + chrono::Duration::minutes(export.delay_minutes as i64);

    // Format UTC date
    let utc_date = delayed_start.format("%b %d, %l:%M%p").to_string().replace("  ", " ");

    // Build title
    let title = build_race_title(transaction, race, export, event_display_name).await;

    // Get volunteers
    let commentators = get_volunteers_for_role(transaction, race, "comment").await.unwrap_or_default();
    let trackers = get_volunteers_for_role(transaction, race, "track").await.unwrap_or_default();

    // Get restream channel
    let restream_channel = race.video_urls
        .get(&backend.language)
        .map(|url| url.to_string())
        .unwrap_or_default();

    // Build notes
    let notes = if is_update {
        "HTH Update: Time changed".to_owned()
    } else {
        String::new()
    };

    // Determine which DST formula to use
    let dst_formula = if backend.language == German {
        if is_german_dst(delayed_start) {
            &backend.dst_formula_dst
        } else {
            &backend.dst_formula_standard
        }
    } else {
        if is_us_eastern_dst(delayed_start) {
            &backend.dst_formula_dst
        } else {
            &backend.dst_formula_standard
        }
    };

    // Find existing row or get next available
    let existing_export = RaceExport::find(transaction, race.id, export.id).await?;

    if let Some(existing) = &existing_export {
        // Update existing row - find row by HTH Export ID
        let id_col_range = format!("'Restream Signups'!{}:{}", backend.hth_export_id_col, backend.hth_export_id_col);
        let id_values = sheets::read_values_uncached(http_client, &backend.google_sheet_id, &id_col_range).await?;

        let mut row_num = None;
        for (idx, row) in id_values.iter().enumerate() {
            if let Some(cell) = row.get(0) {
                if cell == &existing.sheet_row_id {
                    row_num = Some(idx + 1); // 1-indexed
                    break;
                }
            }
        }

        if let Some(row) = row_num {
            // Build batch update
            let mut updates = vec![
                (format!("'Restream Signups'!A{}", row), vec![vec![utc_date]]),
                (format!("'Restream Signups'!B{}", row), vec![vec![format!("=IF(A{}=\"\",\"\",TEXT(A{},\"ddd\"))", row, row)]]),
                (format!("'Restream Signups'!C{}", row), vec![vec![dst_formula.replace("{row}", &row.to_string())]]),
                (format!("'Restream Signups'!D{}", row), vec![vec![format!("=IF(C{}=\"\",\"\",TEXT(C{},\"ddd\"))", row, row)]]),
                (format!("'Restream Signups'!E{}", row), vec![vec![title]]),
                (format!("'Restream Signups'!F{}", row), vec![vec![export.description.clone().unwrap_or_default()]]),
                (format!("'Restream Signups'!G{}", row), vec![vec![get_runner_count(race).to_string()]]),
                (format!("'Restream Signups'!{}{}", backend.commentators_col, row), vec![vec![commentators.join(", ")]]),
                (format!("'Restream Signups'!{}{}", backend.trackers_col, row), vec![vec![trackers.join(", ")]]),
                (format!("'Restream Signups'!{}{}", backend.hth_export_id_col, row), vec![vec![export_id.clone()]]),
                (format!("'Restream Signups'!{}{}", backend.notes_col, row), vec![vec![notes]]),
            ];
            if let Some(ref restream_channel_col) = backend.restream_channel_col {
                updates.push((format!("'Restream Signups'!{}{}", restream_channel_col, row), vec![vec![restream_channel]]));
            }

            sheets::batch_update_values(http_client, &backend.google_sheet_id, updates).await?;
        }
    } else {
        // Append new row to the sheet
        let values = vec![vec![
            utc_date,
            format!("=IF(A4=\"\",\"\",TEXT(A4,\"ddd\"))"),
            dst_formula.replace("{row}", "4"),
            format!("=IF(C4=\"\",\"\",TEXT(C4,\"ddd\"))"),
            title,
            export.description.clone().unwrap_or_default(),
            get_runner_count(race).to_string(),
            String::new(), // Restreamer (column H)
            restream_channel,
            String::new(), // Filler columns
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            commentators.join(", "),
            trackers.join(", "),
            export_id.clone(),
        ]];

        sheets::append_values(
            http_client,
            &backend.google_sheet_id,
            "'Restream Signups'!A:Z",
            values,
        ).await?;
    }

    // Record the export
    RaceExport::upsert(transaction, race.id, export.id, &export_id).await?;

    Ok(export_id)
}

/// Remove a race from a Google Sheet
pub(crate) async fn remove_race(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    race_id: Id<Races>,
    export: &ExportConfig,
    backend: &RestreamingBackend,
) -> Result<(), Error> {
    let existing = RaceExport::find(transaction, race_id, export.id).await?;

    if let Some(existing) = existing {
        // Find the row by HTH Export ID
        let id_col_range = format!("'Restream Signups'!{}:{}", backend.hth_export_id_col, backend.hth_export_id_col);
        let id_values = sheets::read_values_uncached(http_client, &backend.google_sheet_id, &id_col_range).await?;

        for (idx, row) in id_values.iter().enumerate() {
            if let Some(cell) = row.get(0) {
                if cell == &existing.sheet_row_id {
                    let row_num = idx + 1;
                    // Clear the row (set all cells to empty)
                    let updates = vec![
                        (format!("'Restream Signups'!A{}:Z{}", row_num, row_num), vec![vec![String::new(); 26]]),
                    ];
                    sheets::batch_update_values(http_client, &backend.google_sheet_id, updates).await?;
                    break;
                }
            }
        }

        // Remove from tracking
        RaceExport::delete(transaction, race_id, export.id).await?;
    }

    Ok(())
}

/// Sync all races for an export configuration
pub(crate) async fn sync_all_races(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    export: &ExportConfig,
) -> Result<(usize, usize, Vec<String>), Error> {
    let backend = RestreamingBackend::from_id(transaction, export.backend_id).await?
        .ok_or(Error::BackendNotFound(export.backend_id))?;

    let event_data = event::Data::new(transaction, export.series, &export.event).await?
        .ok_or(Error::EventNotFound)?;

    // Get all races for this event
    let race_ids = sqlx::query_scalar!(r#"
        SELECT id AS "id: Id<Races>"
        FROM races
        WHERE series = $1 AND event = $2 AND ignored = false
        ORDER BY start NULLS LAST
    "#, export.series as _, &export.event)
    .fetch_all(&mut **transaction)
    .await?;

    let mut exported = 0;
    let mut removed = 0;
    let mut errors = Vec::new();

    for race_id in race_ids {
        let race = match Race::from_id(transaction, http_client, race_id).await {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("Race {}: {}", race_id, e));
                continue;
            }
        };

        // Check if race should be removed
        if should_remove_race(&race) {
            match remove_race(transaction, http_client, race.id, export, &backend).await {
                Ok(()) => removed += 1,
                Err(e) => errors.push(format!("Race {} (remove): {}", race.id, e)),
            }
            continue;
        }

        // Check if race should be exported
        match should_export_race(transaction, &race, export, &backend).await {
            Ok(true) => {
                // Check if this is an update
                let existing = RaceExport::find(transaction, race.id, export.id).await?;
                let is_update = existing.is_some();

                match export_race(transaction, http_client, &race, export, &backend, &event_data.display_name, is_update).await {
                    Ok(_) => exported += 1,
                    Err(e) => errors.push(format!("Race {}: {}", race.id, e)),
                }
            }
            Ok(false) => {
                // Trigger not met - check if we need to remove
                let existing = RaceExport::find(transaction, race.id, export.id).await?;
                if existing.is_some() {
                    match remove_race(transaction, http_client, race.id, export, &backend).await {
                        Ok(()) => removed += 1,
                        Err(e) => errors.push(format!("Race {} (remove): {}", race.id, e)),
                    }
                }
            }
            Err(e) => errors.push(format!("Race {} (check): {}", race.id, e)),
        }
    }

    Ok((exported, removed, errors))
}

// ============================================================================
// Background Task
// ============================================================================

/// Background task that periodically checks and syncs exports
pub(crate) async fn check_and_sync_all_exports(
    pool: &PgPool,
    http_client: &reqwest::Client,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;

    let exports = ExportConfig::all_enabled(&mut transaction).await?;

    for export in exports {
        match sync_all_races(&mut transaction, http_client, &export).await {
            Ok((exported, removed, errors)) => {
                if exported > 0 || removed > 0 || !errors.is_empty() {
                    eprintln!(
                        "ZSR Export {}/{} to backend {}: exported {}, removed {}, {} errors",
                        export.series.slug(), export.event, export.backend_id,
                        exported, removed, errors.len()
                    );
                    for err in errors {
                        eprintln!("  - {}", err);
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "ZSR Export {}/{} to backend {}: failed - {}",
                    export.series.slug(), export.event, export.backend_id, e
                );
            }
        }
    }

    transaction.commit().await?;
    Ok(())
}
