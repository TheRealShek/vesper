use super::{Database, DbError};

impl Database {
    pub fn query_media(
        &self,
        q: &crate::events::MediaQuery,
    ) -> Result<(Vec<crate::events::UiMediaItem>, u32), DbError> {
        let reader = self.reader.lock().unwrap();

        let mut base_query = String::from("FROM media m");
        let mut where_clauses = Vec::new();
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

        if let Some(search) = &q.search
            && !search.is_empty()
        {
            where_clauses.push(format!("m.filename LIKE ?{}", arg_idx));
            args.push(Box::new(format!("%{}%", search)));
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

        let order_by = match q.sort {
            crate::events::SortOrder::DateModifiedDesc => "ORDER BY m.modified_at DESC, m.id DESC",
            crate::events::SortOrder::DateModifiedAsc => "ORDER BY m.modified_at ASC, m.id ASC",
            crate::events::SortOrder::DateCreatedDesc => "ORDER BY m.created_at DESC, m.id DESC",
            crate::events::SortOrder::DateCreatedAsc => "ORDER BY m.created_at ASC, m.id ASC",
            crate::events::SortOrder::FilenameAsc => "ORDER BY m.filename ASC, m.id ASC",
            crate::events::SortOrder::FilenameDesc => "ORDER BY m.filename DESC, m.id DESC",
            crate::events::SortOrder::FileSizeDesc => "ORDER BY m.size_bytes DESC, m.id DESC",
            crate::events::SortOrder::FileSizeAsc => "ORDER BY m.size_bytes ASC, m.id ASC",
        };

        let select_cols = "m.id, m.path, m.filename, m.source_root_id, m.media_type, \
                           m.size_bytes, m.created_at, m.modified_at, m.thumbnail_path, m.duration_secs, \
                           (SELECT GROUP_CONCAT(tags.name, ',') FROM tags JOIN media_tags ON tags.id = media_tags.tag_id WHERE media_tags.media_id = m.id) AS all_tags";

        let limit_offset = format!("LIMIT {} OFFSET {}", q.limit, q.offset);

        let data_query = format!(
            "SELECT {} {} {} {} {} {}",
            select_cols, base_query, where_sql, group_by, order_by, limit_offset
        );

        let mut stmt = reader.prepare(&data_query)?;

        let offline_roots: std::collections::HashSet<i64> = reader
            .prepare("SELECT id FROM source_roots WHERE is_available = 0")?
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();

        let rows = stmt
            .query_map(rusqlite::params_from_iter(args_ref.iter()), |row| {
                let media_type_str: String = row.get(4)?;
                let media_type = crate::events::MediaType::from_db_str(&media_type_str)
                    .unwrap_or(crate::events::MediaType::Image);

                let root_id: i64 = row.get(3)?;
                let is_offline = offline_roots.contains(&root_id);

                let tags_str: Option<String> = row.get(10)?;

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
                    is_offline,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((rows, total_count))
    }
}
