use crate::RepositoryContext;
use sqlx::Executor;
use sqlx::Postgres;

pub async fn apply_repository_context<'e, E>(
    executor: E,
    context: &RepositoryContext,
) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        select
          set_config('app.current_organization_id', $1, true),
          set_config('app.current_team_id', $2, true),
          set_config('app.current_project_id', $3, true),
          set_config('app.current_user_id', $4, true),
          set_config('app.current_actor_type', $5, true),
          set_config('app.current_team_role', $6, true)
        "#,
    )
    .bind(context.organization_id.to_string())
    .bind(context.team_id.to_string())
    .bind(
        context
            .project_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    )
    .bind(
        context
            .actor_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    )
    .bind(match context.actor_type {
        chum_mem_contracts::ActorType::User => "user",
        chum_mem_contracts::ActorType::Token => "token",
        chum_mem_contracts::ActorType::System => "system",
    })
    .bind(match context.team_role {
        chum_mem_contracts::TeamRole::Owner => "owner",
        chum_mem_contracts::TeamRole::Admin => "admin",
        chum_mem_contracts::TeamRole::Member => "member",
    })
    .execute(executor)
    .await?;

    Ok(())
}
