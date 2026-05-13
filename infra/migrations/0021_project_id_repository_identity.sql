-- Repository identity is project-scoped. Hosted Git repository URLs were
-- legacy metadata and must not participate in project or repository-layer
-- resolution.

alter table public.projects
  drop column if exists repo_url;

alter table public.sessions
  drop column if exists repo_url;
