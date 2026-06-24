use super::{Database, DbError};

impl Database {
    pub fn query_media(
        &self,
        q: &crate::events::MediaQuery,
    ) -> Result<(Vec<crate::events::UiMediaItem>, u32), DbError> {
        let reader = self.reader.lock().unwrap();

        let mut base_query =
            String::from("FROM media m JOIN source_roots sr ON sr.id = m.source_root_id");
        let mut where_clauses = vec![];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut arg_idx = 1;

        if !q.tags.is_empty() {
            base_query.push_str(" JOIN media_tags mt ON m.id = mt.media_id");
            base_query.push_str(" JOIN tags t ON mt.tag_id = t.id");

            let placeholders = (0..q.tags.len())
                .map(|i| format!("?{}", arg_idx + i))
                .collect::<Vec<_>>()
                .join(", ");

            where_clauses.push(format!("t.name IN ({})", placeholders));
            for tag in &q.tags {
                args.push(Box::new(tag.clone()));
            }
            arg_idx += q.tags.len();
        }

        let mut search_like_idx = None;

        if let Some(search) = &q.search
            && !search.is_empty()
        {
            search_like_idx = Some(arg_idx);
            where_clauses.push(format!(
                "(m.filename LIKE ?{0} OR m.path LIKE ?{0} OR EXISTS (SELECT 1 FROM media_tags mt JOIN tags t ON mt.tag_id = t.id WHERE mt.media_id = m.id AND t.name LIKE ?{0}))",
                arg_idx
            ));
            args.push(Box::new(format!("%{}%", search)));
            arg_idx += 1;
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

        let mut args_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();

        let total_count: u32 = reader.query_row(
            &count_query,
            rusqlite::params_from_iter(args_ref.iter()),
            |row| row.get(0),
        )?;

        let mut search_exact_placeholders = None;
        if let Some(search) = &q.search
            && !search.is_empty()
        {
            let mut exact_matches = vec![search.clone()];
            let exts = crate::index::media::IMAGE_EXTENSIONS
                .iter()
                .chain(crate::index::media::VIDEO_EXTENSIONS.iter());

            for ext in exts {
                exact_matches.push(format!("{}.{}", search, ext));
            }

            let mut placeholders = Vec::with_capacity(exact_matches.len());
            for m in exact_matches {
                placeholders.push(format!("?{}", arg_idx));
                args.push(Box::new(m));
                arg_idx += 1;
            }

            search_exact_placeholders = Some(placeholders.join(", "));
            args_ref = args.iter().map(|b| b.as_ref()).collect();
        }

        let order_by_base = match q.sort {
            crate::events::SortOrder::DateModifiedDesc => "m.modified_at DESC, m.id DESC",
            crate::events::SortOrder::DateModifiedAsc => "m.modified_at ASC, m.id ASC",
            crate::events::SortOrder::DateCreatedDesc => "m.created_at DESC, m.id DESC",
            crate::events::SortOrder::DateCreatedAsc => "m.created_at ASC, m.id ASC",
            crate::events::SortOrder::FilenameAsc => "m.filename ASC, m.id ASC",
            crate::events::SortOrder::FilenameDesc => "m.filename DESC, m.id DESC",
            crate::events::SortOrder::FileSizeDesc => "m.size_bytes DESC, m.id DESC",
            crate::events::SortOrder::FileSizeAsc => "m.size_bytes ASC, m.id ASC",
        };

        let order_by = if let (Some(like_idx), Some(placeholders)) =
            (search_like_idx, search_exact_placeholders)
        {
            // We use the same extension list for ranking as we do for indexing.
            // This prevents false positive prefix matches (e.g. 'trip.%' matching 'trip.backup.jpg')
            // without requiring complex SQLite string parsing or schema migrations.
            format!(
                "ORDER BY CASE \
                    WHEN m.filename COLLATE NOCASE IN ({0}) THEN 1 \
                    WHEN EXISTS (SELECT 1 FROM media_tags mt JOIN tags t ON mt.tag_id = t.id WHERE mt.media_id = m.id AND t.name LIKE ?{1}) THEN 2 \
                    ELSE 3 \
                 END ASC, {2}",
                placeholders, like_idx, order_by_base
            )
        } else {
            format!("ORDER BY {}", order_by_base)
        };

        let select_cols = "m.id, m.path, m.filename, m.source_root_id, m.media_type, \
                           m.size_bytes, m.created_at, m.modified_at, m.thumbnail_path, m.duration_secs, \
                           (SELECT GROUP_CONCAT(tags.name, ',') FROM tags JOIN media_tags ON tags.id = media_tags.tag_id WHERE media_tags.media_id = m.id) AS all_tags, \
                           sr.is_available";

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
    use crate::db::{Database, MediaEntry};
    use crate::events::{MediaQuery, MediaType, SortOrder, TagMode};

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
}
