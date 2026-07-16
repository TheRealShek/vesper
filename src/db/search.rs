use super::{Database, DbError};

impl Database {
    pub fn query_media(
        &self,
        q: &crate::events::MediaQuery,
    ) -> Result<(Vec<crate::events::UiMediaItem>, u32), DbError> {
        let reader = self.lock_reader()?;

        let mut base_query =
            String::from("FROM media m JOIN source_roots sr ON sr.id = m.source_root_id");
        // NEW-1: offline-root media stays in SQLite but is excluded from every
        // published result — grid, search, selection, and viewer all consume
        // this query.
        let mut where_clauses = vec!["sr.is_available = 1".to_string()];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut arg_idx = 1;

        if !q.tags.is_empty() {
            base_query.push_str(" JOIN media_tags mt ON m.id = mt.media_id");
            base_query.push_str(" JOIN tags t ON mt.tag_id = t.id");

            let mut identities = Vec::with_capacity(q.tags.len());
            for tag in &q.tags {
                identities.push(format!(
                    "(t.source_root_id = ?{} AND t.relative_folder_path = ?{})",
                    arg_idx,
                    arg_idx + 1
                ));
                args.push(Box::new(tag.source_root_id));
                args.push(Box::new(tag.relative_folder_path.clone()));
                arg_idx += 2;
            }
            where_clauses.push(format!("({})", identities.join(" OR ")));
        }

        let normalized_search = q
            .search
            .as_deref()
            .map(str::trim)
            .map(super::search_normalization::normalize_search_text)
            .filter(|search| !search.is_empty());
        let mut search_idx = None;

        if let Some(search) = &normalized_search {
            search_idx = Some(arg_idx);
            where_clauses.push(format!(
                "(instr(m.basename_search, ?{0}) > 0 OR instr(m.path_search, ?{0}) > 0 OR \
                  EXISTS (SELECT 1 FROM media_tags search_mt \
                          JOIN tags search_t ON search_mt.tag_id = search_t.id \
                         WHERE search_mt.media_id = m.id \
                           AND (instr(search_t.display_name_search, ?{0}) > 0 \
                                OR instr(search_t.display_path_search, ?{0}) > 0)))",
                arg_idx
            ));
            args.push(Box::new(search.clone()));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let group_by = if !q.tags.is_empty() {
            if q.tag_mode == crate::events::TagMode::All {
                format!(
                    "GROUP BY m.id HAVING COUNT(DISTINCT t.id) = {}",
                    q.tags.len()
                )
            } else {
                "GROUP BY m.id".to_string()
            }
        } else {
            String::new()
        };

        let count_query = format!(
            "SELECT COUNT(*) FROM (SELECT m.id {} {} {})",
            base_query, where_sql, group_by
        );

        let args_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();

        let total_count: u32 = reader.query_row(
            &count_query,
            rusqlite::params_from_iter(args_ref.iter()),
            |row| row.get(0),
        )?;

        let order_by_base = match q.sort {
            crate::events::SortOrder::DateModifiedDesc => "m.modified_at DESC",
            crate::events::SortOrder::DateModifiedAsc => "m.modified_at ASC",
            crate::events::SortOrder::DateAddedDesc => "m.date_added DESC",
            crate::events::SortOrder::DateAddedAsc => "m.date_added ASC",
            crate::events::SortOrder::FilenameAsc => "m.filename ASC",
            crate::events::SortOrder::FilenameDesc => "m.filename DESC",
            crate::events::SortOrder::FileSizeDesc => "m.size_bytes DESC",
            crate::events::SortOrder::FileSizeAsc => "m.size_bytes ASC",
        };

        let order_by = if let Some(search_idx) = search_idx {
            format!(
                "ORDER BY CASE \
                    WHEN m.basename_search = ?{0} THEN 1 \
                    WHEN instr(m.basename_search, ?{0}) = 1 THEN 2 \
                    WHEN instr(m.basename_search, ?{0}) > 0 THEN 3 \
                    WHEN EXISTS (SELECT 1 FROM media_tags rank_mt \
                                 JOIN tags rank_t ON rank_mt.tag_id = rank_t.id \
                                WHERE rank_mt.media_id = m.id \
                                  AND (rank_t.display_name_search = ?{0} \
                                       OR rank_t.display_path_search = ?{0})) THEN 4 \
                    WHEN EXISTS (SELECT 1 FROM media_tags rank_mt \
                                 JOIN tags rank_t ON rank_mt.tag_id = rank_t.id \
                                WHERE rank_mt.media_id = m.id \
                                  AND (instr(rank_t.display_name_search, ?{0}) > 0 \
                                       OR instr(rank_t.display_path_search, ?{0}) > 0)) THEN 5 \
                    WHEN instr(m.path_search, ?{0}) > 0 THEN 6 \
                    ELSE 7 \
                 END ASC, {1}, m.path ASC",
                search_idx, order_by_base
            )
        } else {
            format!("ORDER BY {}, m.path ASC", order_by_base)
        };

        let select_cols = "m.id, m.path, m.filename, m.source_root_id, m.media_type, \
                           m.size_bytes, m.created_at, m.modified_at, m.thumbnail_path, m.duration_secs, \
                           (SELECT GROUP_CONCAT(tags.display_name, ',') FROM tags JOIN media_tags ON tags.id = media_tags.tag_id WHERE media_tags.media_id = m.id) AS all_tags, \
                           sr.is_available, m.date_added";

        let data_query = format!(
            "SELECT {} {} {} {} {}",
            select_cols, base_query, where_sql, group_by, order_by
        );

        let mut stmt = reader.prepare(&data_query)?;

        let rows = stmt
            .query_map(rusqlite::params_from_iter(args_ref.iter()), |row| {
                let media_type_str: String = row.get(4)?;
                let media_type = crate::events::MediaType::from_db_str(&media_type_str)
                    .unwrap_or(crate::events::MediaType::Image);

                let tags_str: Option<String> = row.get(10)?;
                let is_available: bool = row.get(11)?;

                Ok(crate::events::UiMediaItem {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    tags: tags_str.unwrap_or_default(),
                    thumbnail_path: row.get(8).unwrap_or_default(),
                    duration_secs: row.get(9).unwrap_or(-1),
                    media_type,
                    size_bytes: row.get(5)?,
                    created_at: row.get(6)?,
                    date_added: row.get(12)?,
                    modified_at: row.get(7)?,
                    is_offline: !is_available,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((rows, total_count))
    }
}

#[cfg(test)]
mod tests {
    use crate::db::{Database, MediaEntry, TagIdentity};
    use crate::events::{MediaQuery, MediaType, SortOrder, TagMode};
    use crate::state::TagFilter;

    fn setup_test_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.add_source_root("/media", "/media").unwrap();

        let files = vec![
            "trip.jpg",
            "trip.png",
            "trip.backup.jpg",
            "trip.v1.backup.jpg",
            "my-trip.jpg",
        ];

        {
            let writer = db.writer.lock().unwrap();
            for (i, file) in files.into_iter().enumerate() {
                let entry = MediaEntry {
                    path: format!("/media/{}", file),
                    relative_path: file.to_string(),
                    canonical_identity: format!("/media/{}", file),
                    filename: file.to_string(),
                    source_root_id: 1,
                    media_type: MediaType::Image,
                    size_bytes: 1000,
                    created_at: None,
                    modified_at: 1000 + i as i64,
                };
                db.upsert_media_inner(&writer, &entry, 1).unwrap();
            }
        }

        db
    }

    #[test]
    fn test_exact_filename_ranking_trip() {
        let db = setup_test_db();

        let q = MediaQuery {
            tags: vec![],
            tag_mode: TagMode::Any,
            search: Some("trip".to_string()),
            sort: SortOrder::DateModifiedAsc,
        };

        let (results, _) = db.query_media(&q).unwrap();

        assert_eq!(results.len(), 5);

        assert_eq!(results[0].filename, "trip.jpg");
        assert_eq!(results[1].filename, "trip.png");
        assert_eq!(results[2].filename, "trip.backup.jpg");
        assert_eq!(results[3].filename, "trip.v1.backup.jpg");
        assert_eq!(results[4].filename, "my-trip.jpg");
    }

    #[test]
    fn test_exact_filename_ranking_trip_backup() {
        let db = setup_test_db();

        let q = MediaQuery {
            tags: vec![],
            tag_mode: TagMode::Any,
            search: Some("trip.backup".to_string()),
            sort: SortOrder::DateModifiedAsc,
        };

        let (results, _) = db.query_media(&q).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "trip.backup.jpg");
    }

    #[test]
    fn date_added_sort_orders_both_directions_with_path_tiebreaker() {
        let db = setup_test_db();
        {
            let writer = db.writer.lock().unwrap();
            for (filename, date_added) in [
                ("trip.jpg", 3_000),
                ("trip.png", 1_000),
                ("trip.backup.jpg", 2_000),
                ("trip.v1.backup.jpg", 2_000),
                ("my-trip.jpg", 4_000),
            ] {
                writer
                    .execute(
                        "UPDATE media SET date_added = ?1 WHERE filename = ?2",
                        rusqlite::params![date_added, filename],
                    )
                    .unwrap();
            }
        }

        let query = |sort| MediaQuery {
            tags: vec![],
            tag_mode: TagMode::Any,
            search: None,
            sort,
        };

        let (newest, _) = db.query_media(&query(SortOrder::DateAddedDesc)).unwrap();
        let newest_names: Vec<&str> = newest.iter().map(|item| item.filename.as_str()).collect();
        assert_eq!(
            newest_names,
            [
                "my-trip.jpg",
                "trip.jpg",
                "trip.backup.jpg",
                "trip.v1.backup.jpg",
                "trip.png",
            ]
        );

        let (oldest, _) = db.query_media(&query(SortOrder::DateAddedAsc)).unwrap();
        let oldest_names: Vec<&str> = oldest.iter().map(|item| item.filename.as_str()).collect();
        assert_eq!(
            oldest_names,
            [
                "trip.png",
                "trip.backup.jpg",
                "trip.v1.backup.jpg",
                "trip.jpg",
                "my-trip.jpg",
            ]
        );
    }

    fn add_tagged(db: &Database, file: &str, tag: &str, mtime: i64) {
        let entry = MediaEntry {
            path: format!("/media/{file}"),
            relative_path: file.to_string(),
            canonical_identity: format!("/media/{file}"),
            filename: file.to_string(),
            source_root_id: 1,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: mtime,
        };
        let media_id = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap().unwrap()
        };
        db.sync_tags_for_media(
            media_id,
            &[TagIdentity {
                source_root_id: 1,
                relative_folder_path: tag.to_string(),
                display_name: tag.to_string(),
                display_path: tag.to_string(),
            }],
        )
        .unwrap();
    }

    fn add_search_item(db: &Database, path: &str, filename: &str, tag: Option<&str>, mtime: i64) {
        let entry = MediaEntry {
            path: path.to_string(),
            relative_path: path.trim_start_matches("/media/").to_string(),
            canonical_identity: path.to_string(),
            filename: filename.to_string(),
            source_root_id: 1,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: mtime,
        };
        let media_id = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap().unwrap()
        };
        if let Some(tag) = tag {
            db.sync_tags_for_media(
                media_id,
                &[TagIdentity {
                    source_root_id: 1,
                    relative_folder_path: format!("tags/{tag}"),
                    display_name: tag.to_string(),
                    display_path: tag.to_string(),
                }],
            )
            .unwrap();
        }
    }

    #[test]
    fn search_ranking_orders_all_eight_contract_levels() {
        let db = Database::open_in_memory().unwrap();
        db.add_source_root("/media", "/media").unwrap();

        add_search_item(&db, "/media/exact/Japan.jpg", "Japan.jpg", None, 1);
        add_search_item(
            &db,
            "/media/prefix/Japan_Trip.jpg",
            "Japan_Trip.jpg",
            None,
            1,
        );
        add_search_item(
            &db,
            "/media/substring/My_Japan_Photo.jpg",
            "My_Japan_Photo.jpg",
            None,
            1,
        );
        add_search_item(
            &db,
            "/media/tag-exact.jpg",
            "tag-exact.jpg",
            Some("Japan"),
            1,
        );
        add_search_item(
            &db,
            "/media/tag-substring.jpg",
            "tag-substring.jpg",
            Some("Historic Japan Collection"),
            1,
        );
        add_search_item(&db, "/media/Japan/z-new.jpg", "z-new.jpg", None, 200);
        add_search_item(&db, "/media/Japan/a-old.jpg", "a-old.jpg", None, 100);
        add_search_item(&db, "/media/Japan/b-old.jpg", "b-old.jpg", None, 100);

        let query = MediaQuery {
            tags: vec![],
            tag_mode: TagMode::Any,
            search: Some("japan".to_string()),
            sort: SortOrder::DateModifiedDesc,
        };
        let (results, _) = db.query_media(&query).unwrap();
        let paths: Vec<&str> = results.iter().map(|item| item.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/media/exact/Japan.jpg",
                "/media/prefix/Japan_Trip.jpg",
                "/media/substring/My_Japan_Photo.jpg",
                "/media/tag-exact.jpg",
                "/media/tag-substring.jpg",
                "/media/Japan/z-new.jpg",
                "/media/Japan/a-old.jpg",
                "/media/Japan/b-old.jpg",
            ]
        );
    }

    #[test]
    fn unicode_search_matches_composed_decomposed_and_mixed_case() {
        let db = Database::open_in_memory().unwrap();
        db.add_source_root("/media", "/media").unwrap();
        add_search_item(&db, "/media/CAFE\u{301}.jpg", "CAFE\u{301}.jpg", None, 1);

        let query = MediaQuery {
            tags: vec![],
            tag_mode: TagMode::Any,
            search: Some("  cAfÉ  ".to_string()),
            sort: SortOrder::DateModifiedDesc,
        };
        let (results, _) = db.query_media(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "CAFE\u{301}.jpg");
    }

    #[test]
    fn search_tie_breaks_same_tier_by_full_path_ascending() {
        let db = Database::open_in_memory().unwrap();
        db.add_source_root("/media", "/media").unwrap();
        add_search_item(&db, "/media/b/Japan.jpg", "Japan.jpg", None, 100);
        add_search_item(&db, "/media/a/Japan.jpg", "Japan.jpg", None, 100);

        let query = MediaQuery {
            tags: vec![],
            tag_mode: TagMode::Any,
            search: Some("japan".to_string()),
            sort: SortOrder::DateModifiedDesc,
        };
        let (results, _) = db.query_media(&query).unwrap();
        let paths: Vec<&str> = results.iter().map(|item| item.path.as_str()).collect();
        assert_eq!(paths, ["/media/a/Japan.jpg", "/media/b/Japan.jpg"]);
    }

    fn filter(root: i64, path: &str, name: &str) -> TagFilter {
        TagFilter {
            source_root_id: root,
            relative_folder_path: path.to_string(),
            display_name: name.to_string(),
        }
    }

    #[test]
    fn selecting_duplicate_tag_name_filters_by_full_identity() {
        let db = Database::open_in_memory().unwrap();
        let root_a = db.add_source_root("/media-a", "/media-a").unwrap();
        let root_b = db.add_source_root("/media-b", "/media-b").unwrap();

        for (root, root_path, lineage, file) in [
            (root_a, "/media-a", "Travel/2023", "a.jpg"),
            (root_b, "/media-b", "Archive/2023", "b.jpg"),
        ] {
            let entry = MediaEntry {
                path: format!("{root_path}/{file}"),
                relative_path: file.to_string(),
                canonical_identity: format!("{root_path}/{file}"),
                filename: file.to_string(),
                source_root_id: root,
                media_type: MediaType::Image,
                size_bytes: 1,
                created_at: None,
                modified_at: 1,
            };
            let media_id = {
                let writer = db.writer.lock().unwrap();
                db.upsert_media_inner(&writer, &entry, 1).unwrap().unwrap()
            };
            db.sync_tags_for_media(
                media_id,
                &[TagIdentity {
                    source_root_id: root,
                    relative_folder_path: lineage.to_string(),
                    display_name: "2023".to_string(),
                    display_path: lineage.to_string(),
                }],
            )
            .unwrap();
        }

        let query = MediaQuery {
            tags: vec![filter(root_a, "Travel/2023", "2023")],
            tag_mode: TagMode::Any,
            search: None,
            sort: SortOrder::FilenameAsc,
        };
        let (results, _) = db.query_media(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "a.jpg");
    }

    #[test]
    fn live_delta_refreshes_active_filtered_query_respecting_the_filter() {
        // B-2 / ARCH-002: a live filesystem delta must be evaluated against the
        // active query by re-running it (a superseding refresh), never applied
        // blind to the on-screen list. Here the active query is a tag filter.
        let db = Database::open_in_memory().unwrap();
        db.add_source_root("/media", "/media").unwrap();
        add_tagged(&db, "t1.jpg", "Travel", 1000);
        add_tagged(&db, "w1.jpg", "Work", 1001);

        let active = MediaQuery {
            tags: vec![filter(1, "Travel", "Travel")],
            tag_mode: TagMode::Any,
            search: None,
            sort: SortOrder::DateModifiedAsc,
        };

        // Before the delta, the filtered query shows only the Travel item.
        let (before, _) = db.query_media(&active).unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].filename, "t1.jpg");

        // A live delta arrives: a new Travel file and a new Work file appear.
        add_tagged(&db, "t2.jpg", "Travel", 1002);
        add_tagged(&db, "w2.jpg", "Work", 1003);

        // Re-running the active query (the superseding refresh) incorporates the
        // matching delta and still excludes the non-matching Work items — the
        // filter is honoured rather than the grid being mutated blind.
        let (after, _) = db.query_media(&active).unwrap();
        let names: Vec<&str> = after.iter().map(|m| m.filename.as_str()).collect();
        assert_eq!(after.len(), 2);
        assert!(names.contains(&"t1.jpg") && names.contains(&"t2.jpg"));
        assert!(
            !names.iter().any(|n| n.starts_with('w')),
            "the refresh must respect the active filter, not add unmatched items"
        );
    }
}
