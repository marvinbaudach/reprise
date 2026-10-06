use {rusqlite::Connection, std::path::Path};

struct MigrationContext<'a> {
    existing_database: bool,
    cover_cache: &'a Path,
    portrait_cache: &'a Path,
}

struct Migration {
    version: i64,
    run: fn(&Connection, &MigrationContext<'_>) -> Result<(), rusqlite::Error>,
}

macro_rules! migration {
    ($version:literal, $run:path) => {
        Migration {
            version: $version,
            run: |conn, _| $run(conn),
        }
    };
}

const MIGRATIONS: &[Migration] = &[
    migration!(19, crate::db_library_doctor::migrate_v19),
    migration!(20, crate::db_tag_write_jobs::migrate_v20),
    migration!(21, crate::db_library_doctor_remote::migrate_v21),
    migration!(22, crate::db_library_doctor_remote::migrate_v22),
    migration!(23, crate::db_mix_planner::migrate_v23),
    migration!(24, crate::db_listen_history::migrate_v24),
    migration!(25, crate::db_library_exclusions::migrate_v25),
    migration!(26, crate::db_new_releases_history::migrate_v26),
    migration!(27, crate::db_drop_audio_analysis_mix::migrate_v27),
    migration!(28, crate::db_change_log::migrate_v28),
    migration!(29, crate::db_ai_jobs::migrate_v29),
    migration!(30, crate::db_artist_news_fetch::migrate_v30),
    migration!(31, crate::db_concerts::migrate_v31),
    migration!(32, crate::db_podcasts_radio::migrate_v32),
    migration!(33, crate::db_podcasts_radio::migrate_v33),
    migration!(34, crate::db_podcasts_radio::migrate_v34),
    migration!(35, crate::db_recently_added::migrate_v35),
    migration!(36, crate::db_device_sync::migrate_v36),
    migration!(37, crate::db_device_sync::migrate_v37),
    migration!(38, crate::db_device_sync::migrate_v38),
    migration!(39, crate::db_release_discography::migrate_v39),
    migration!(40, crate::db_podcasts_radio::migrate_v40),
    migration!(41, crate::db_podcasts_radio::migrate_v41),
    migration!(42, crate::db_device_sync::migrate_v42),
    migration!(43, crate::db_podcasts_radio::migrate_v43),
    migration!(44, crate::db_device_sync::migrate_v44),
    migration!(45, crate::db_sync_log::migrate_v45),
    migration!(46, crate::db_device_sync::migrate_v46),
    migration!(47, crate::db_podcasts_radio::migrate_v47),
    migration!(48, crate::db_podcasts_radio::migrate_v48),
    migration!(49, crate::db_podcasts_radio::migrate_v49),
    Migration {
        version: 50,
        run: |c, ctx| {
            crate::db_online_sources::migrate_v50(
                c,
                ctx.existing_database,
                ctx.cover_cache,
                ctx.portrait_cache,
            )
        },
    },
    migration!(51, crate::db_podcasts_radio::migrate_v51),
    migration!(52, crate::db_podcasts_radio::migrate_v52),
    migration!(53, crate::db_equalizer::migrate_v53),
    migration!(54, crate::db_play_journal::migrate_v54),
    migration!(55, crate::db_spectrogram::migrate_v55),
    migration!(56, crate::db_new_releases_accent::migrate_v56),
    migration!(57, crate::db_drop_sound_features::migrate_v57),
    migration!(58, crate::db_library_doctor::migrate_v58),
    migration!(59, crate::db_podcasts_radio::migrate_v59),
    migration!(60, crate::db_drop_sound_features::migrate_v60),
    migration!(61, crate::db_mobile_sync::migrate_v61),
    migration!(62, crate::db_releases_view_scope::migrate_v62),
    migration!(63, crate::db_listens_back::migrate_v63),
    migration!(64, crate::db_mobile_sync::migrate_v64),
    migration!(65, crate::db_listens_back::migrate_v65),
    migration!(66, crate::db_library_doctor::migrate_v66),
    migration!(67, crate::db_library_doctor::migrate_v67),
    migration!(68, crate::db_device_sync::migrate_v68),
    migration!(69, crate::db_deleted_releases::migrate_v69),
    migration!(70, crate::db_deleted_releases::migrate_v70),
    migration!(71, crate::db_artwork::migrate_v71),
    migration!(72, crate::db_artwork::migrate_v72),
    migration!(73, crate::db_concerts::migrate_v73),
    migration!(74, crate::db_new_releases_notify::migrate_v74),
    migration!(75, crate::db_concerts::migrate_v75),
    migration!(76, crate::db_concerts::migrate_v76),
    migration!(77, crate::db_podcast_channel_image::migrate_v77),
    migration!(78, crate::db_podcast_resume_scope::migrate_v78),
    migration!(79, crate::library::settings::migrate_v79),
    migration!(80, crate::library::settings::migrate_v80),
    migration!(81, crate::db_sync_log::migrate_v81),
    migration!(82, crate::db_sort_indexes::migrate_v82),
    migration!(83, crate::db_cover_download::migrate_v83),
    migration!(84, crate::db_cover_download::migrate_v84),
    migration!(85, crate::db_smart_playlist_names::migrate_v85),
    migration!(86, crate::db_library_doctor::migrate_v86),
    migration!(87, crate::db_device_sync::migrate_v87),
    migration!(88, crate::db_playlist_track_index::migrate_v88),
    migration!(89, crate::library::loudness_store::migrate_v89),
    migration!(90, crate::db_cue_segments::migrate_v90),
    migration!(91, crate::db_cue_wave3::migrate_v91),
];

pub const SUPPORTED_SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].version;

pub(crate) fn run_migrations(
    conn: &Connection,
    existing_database: bool,
    cover_cache: &Path,
    portrait_cache: &Path,
) -> Result<(), rusqlite::Error> {
    let context = MigrationContext {
        existing_database,
        cover_cache,
        portrait_cache,
    };
    for migration in MIGRATIONS {
        (migration.run)(conn, &context)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn migration_versions() -> Vec<i64> {
    MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .collect()
}

/// Applies only the migrations up to and including `last_version`, so a test
/// can build the schema as it was before a later migration and then run that
/// one on rows of its own.
#[cfg(test)]
pub(crate) fn run_migrations_through(
    conn: &Connection,
    last_version: i64,
) -> Result<(), rusqlite::Error> {
    let scratch = tempfile::tempdir().expect("scratch cache directory");
    let context = MigrationContext {
        existing_database: false,
        cover_cache: scratch.path(),
        portrait_cache: scratch.path(),
    };
    for migration in MIGRATIONS
        .iter()
        .take_while(|migration| migration.version <= last_version)
    {
        (migration.run)(conn, &context)?;
    }
    Ok(())
}
