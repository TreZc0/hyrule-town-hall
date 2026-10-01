use {super::*, crate::id::RoleBindings};

pub(crate) struct RoleChoice {
    pub(crate) id: Id<RoleBindings>,
    pub(crate) name: String,
    pub(crate) language: Language,
}

#[derive(Default)]
pub(crate) struct RoleSelection {
    // None includes future bindings too; Some(empty) must never expand to all roles.
    ids: Option<Vec<Id<RoleBindings>>>,
}

impl RoleSelection {
    pub(crate) fn parse(
        mode: &str,
        ids: &[Id<RoleBindings>],
        choices: &[RoleChoice],
        language: Language,
    ) -> Result<Self, &'static str> {
        match mode {
            "all" => Ok(Self::default()),
            "selected"
                if !ids.is_empty()
                    && ids.iter().all(|id| {
                        choices
                            .iter()
                            .any(|choice| choice.id == *id && choice.language == language)
                    }) =>
            {
                let mut ids = ids.to_vec();
                ids.sort();
                ids.dedup();
                Ok(Self { ids: Some(ids) })
            }
            _ => Err("Select at least one available role in the workflow's language."),
        }
    }

    pub(crate) async fn load(
        transaction: &mut Transaction<'_, Postgres>,
        workflow_id: i32,
    ) -> sqlx::Result<Self> {
        let ids: Option<sqlx::types::Json<Vec<Id<RoleBindings>>>> = sqlx::query_scalar(
            "SELECT role_binding_ids FROM volunteer_ping_workflows WHERE id = $1",
        )
        .bind(workflow_id)
        .fetch_one(&mut **transaction)
        .await?;
        Ok(Self {
            ids: ids.map(|ids| ids.0),
        })
    }

    pub(crate) async fn save(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        workflow_id: i32,
    ) -> sqlx::Result<()> {
        sqlx::query("UPDATE volunteer_ping_workflows SET role_binding_ids = $1 WHERE id = $2")
            .bind(self.ids.as_ref().map(sqlx::types::Json))
            .bind(workflow_id)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    pub(crate) fn includes(&self, binding: &EffectiveRoleBinding, language: Language) -> bool {
        !binding.is_disabled
            && binding.language == language
            && self
                .ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&binding.id))
    }

    pub(crate) fn label(&self, choices: &[RoleChoice], language: Language) -> String {
        match &self.ids {
            None => "All roles in this language".into(),
            Some(ids) => {
                let names: Vec<_> = choices
                    .iter()
                    .filter(|choice| choice.language == language && ids.contains(&choice.id))
                    .map(|choice| choice.name.as_str())
                    .collect();
                if names.is_empty() {
                    "No active selected roles".into()
                } else {
                    names.join(", ")
                }
            }
        }
    }

    pub(crate) fn fields(
        &self,
        choices: &[RoleChoice],
        language: Option<Language>,
        language_field: &str,
    ) -> RawHtml<String> {
        html! {
            fieldset(class = "ping-role-selection", data_role_language = language_field) {
                legend : "Roles to ping";
                p(class = "ping-role-help") : "Give commentators, trackers, or other roles their own ping schedule by choosing specific roles.";
                select(name = "role_selection", aria_label = "Roles to ping") {
                    option(value = "all", selected? = self.ids.is_none()) : "All roles in this language";
                    option(value = "selected", selected? = self.ids.is_some()) : "Choose specific roles…";
                }
                div(class = "ping-role-choices") {
                    @for choice in choices.iter().filter(|choice| language.is_none_or(|language| choice.language == language)) {
                        label(data_role_language = choice.language.short_code()) {
                            input(type = "checkbox", name = "role_binding_ids", value = choice.id.to_string(),
                                checked? = self.ids.as_ref().is_some_and(|ids| ids.contains(&choice.id)));
                            : &choice.name;
                        }
                    }
                    p(class = "ping-role-help") : "Only selected roles with unfilled volunteer slots will be requested.";
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(id: i64, language: Language, is_game_binding: bool) -> EffectiveRoleBinding {
        EffectiveRoleBinding {
            id: Id::from(id),
            role_type_id: Id::from(id),
            min_count: 1,
            max_count: 2,
            role_type_name: format!("Role {id}"),
            discord_role_id: Some(100 + id),
            auto_approve: false,
            is_game_binding,
            has_event_override: is_game_binding,
            is_disabled: false,
            language,
        }
    }

    fn choices() -> Vec<RoleChoice> {
        vec![
            RoleChoice {
                id: Id::from(1_i64),
                name: "Commentators".into(),
                language: English,
            },
            RoleChoice {
                id: Id::from(2_i64),
                name: "Trackers".into(),
                language: English,
            },
            RoleChoice {
                id: Id::from(3_i64),
                name: "French commentators".into(),
                language: French,
            },
        ]
    }

    #[test]
    fn selected_roles_include_inherited_bindings_but_exclude_other_roles_and_languages() {
        let selection =
            RoleSelection::parse("selected", &[Id::from(1_i64)], &choices(), English).unwrap();
        assert!(selection.includes(&binding(1, English, true), English));
        assert!(selection.includes(&binding(1, English, false), English));
        assert!(!selection.includes(&binding(2, English, true), English));
        assert!(!selection.includes(&binding(1, French, true), English));
        let mut disabled = binding(1, English, true);
        disabled.is_disabled = true;
        assert!(!selection.includes(&disabled, English));
        assert_eq!(selection.label(&choices(), English), "Commentators");
    }

    #[test]
    fn all_roles_include_future_bindings_but_respect_language_and_disabled_status() {
        let selection = RoleSelection::default();
        assert!(selection.includes(&binding(99, English, false), English));
        assert!(!selection.includes(&binding(99, French, false), English));
        let mut disabled = binding(99, English, true);
        disabled.is_disabled = true;
        assert!(!selection.includes(&disabled, English));
    }

    #[test]
    fn rejects_empty_unavailable_wrong_language_and_invalid_selections() {
        for ids in [vec![], vec![Id::from(99_i64)], vec![Id::from(3_i64)]] {
            assert!(RoleSelection::parse("selected", &ids, &choices(), English).is_err());
        }
        assert!(RoleSelection::parse("invalid", &[], &choices(), English).is_err());
        let selection = RoleSelection::parse(
            "selected",
            &[Id::from(1_i64), Id::from(1_i64)],
            &choices(),
            English,
        )
        .unwrap();
        assert_eq!(selection.ids.unwrap().len(), 1);
    }

    #[test]
    fn removing_all_selected_bindings_never_falls_back_to_all_roles() {
        let selection = RoleSelection {
            ids: Some(Vec::new()),
        };
        assert!(!selection.includes(&binding(1, English, true), English));
        assert_eq!(
            selection.label(&choices(), English),
            "No active selected roles"
        );
    }

    #[tokio::test]
    #[ignore = "requires PING_TEST_DATABASE_URL pointing to a migrated test database"]
    async fn role_selection_database_round_trip_and_deletion() -> sqlx::Result<()> {
        let pool =
            PgPool::connect(&std::env::var("PING_TEST_DATABASE_URL").expect("test database URL"))
                .await?;
        let mut transaction = pool.begin().await?;
        let game_id: i32 = sqlx::query_scalar("INSERT INTO games (name, display_name) VALUES ('ping-role-test', 'Ping role test') RETURNING id")
            .fetch_one(&mut *transaction).await?;
        let role_type: i32 = sqlx::query_scalar(
            "INSERT INTO role_types (name) VALUES ('Ping role test') RETURNING id",
        )
        .fetch_one(&mut *transaction)
        .await?;
        let binding_id: i32 = sqlx::query_scalar(
            "INSERT INTO role_bindings (game_id, role_type_id) VALUES ($1, $2) RETURNING id",
        )
        .bind(game_id)
        .bind(role_type)
        .fetch_one(&mut *transaction)
        .await?;
        let workflow_id: i32 = sqlx::query_scalar("INSERT INTO volunteer_ping_workflows (game_id, workflow_type) VALUES ($1, 'per_race') RETURNING id")
            .bind(game_id).fetch_one(&mut *transaction).await?;
        assert!(
            RoleSelection::load(&mut transaction, workflow_id)
                .await?
                .ids
                .is_none()
        );

        let selected = RoleSelection {
            ids: Some(vec![Id::from(i64::from(binding_id))]),
        };
        selected.save(&mut transaction, workflow_id).await?;
        assert_eq!(
            RoleSelection::load(&mut transaction, workflow_id)
                .await?
                .ids,
            selected.ids
        );
        RoleSelection::default()
            .save(&mut transaction, workflow_id)
            .await?;
        assert!(
            RoleSelection::load(&mut transaction, workflow_id)
                .await?
                .ids
                .is_none()
        );
        selected.save(&mut transaction, workflow_id).await?;
        sqlx::query("DELETE FROM role_bindings WHERE id = $1")
            .bind(binding_id)
            .execute(&mut *transaction)
            .await?;
        let selection = RoleSelection::load(&mut transaction, workflow_id).await?;
        assert_eq!(selection.ids, selected.ids);
        assert_eq!(selection.label(&[], English), "No active selected roles");
        assert!(!selection.includes(&binding(99, English, true), English));
        transaction.rollback().await?;
        Ok(())
    }
}
