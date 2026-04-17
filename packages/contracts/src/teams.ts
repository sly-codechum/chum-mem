import { z } from 'zod';
import { timestampSchema } from './common.js';

export const teamMembershipSchema = z.object({
  organizationId: z.string().uuid(),
  teamId: z.string().uuid(),
  teamName: z.string().min(1),
  teamSlug: z.string().min(1),
  role: z.enum(['owner', 'admin', 'member']),
  status: z.enum(['active', 'invited', 'suspended']),
  createdAt: timestampSchema
});
export type TeamMembership = z.infer<typeof teamMembershipSchema>;

export const listMyTeamsResponseSchema = z.object({
  teams: z.array(teamMembershipSchema)
});
export type ListMyTeamsResponse = z.infer<typeof listMyTeamsResponseSchema>;

export const projectSchema = z.object({
  id: z.string().uuid(),
  organizationId: z.string().uuid(),
  teamId: z.string().uuid(),
  name: z.string().min(1),
  slug: z.string().min(1),
  repoUrl: z.string().url().nullable(),
  defaultBranch: z.string().nullable(),
  createdAt: timestampSchema
});
export type Project = z.infer<typeof projectSchema>;

export const listProjectsRequestSchema = z.object({
  teamId: z.string().uuid(),
  projectId: z.string().uuid().optional()
});
export type ListProjectsRequest = z.infer<typeof listProjectsRequestSchema>;

export const listProjectsResponseSchema = z.object({
  projects: z.array(projectSchema)
});
export type ListProjectsResponse = z.infer<typeof listProjectsResponseSchema>;
