import { createShow } from 'formajs';

/**
 * Role hierarchy for Organization Members.
 * viewer < admin < owner
 */
const ROLE_LEVEL: Record<string, number> = { viewer: 0, admin: 1, owner: 2 };

export interface RoleGuardProps {
  /** The current user's role -- passed from the surface store */
  role: () => string;
  /** Minimum role required to render children */
  minimum: 'viewer' | 'admin' | 'owner';
  /** Content to render when role meets minimum */
  children: () => any;
  /** Optional fallback when role is insufficient */
  fallback?: () => any;
}

/**
 * Declarative role-based access control wrapper.
 * Renders children only if the user's role meets the minimum required.
 *
 * The guard takes `role` as a prop (signal accessor) rather than reading
 * from a global store. This is because different surfaces may get the role
 * from different stores -- the page/view passes it in.
 *
 * Usage:
 *   h(RoleGuard, { role: () => store.role(), minimum: 'admin' },
 *     () => h(Button, { label: 'Delete', variant: 'danger' })
 *   )
 */
export function RoleGuard(props: RoleGuardProps) {
  return createShow(
    () => (ROLE_LEVEL[props.role()] ?? -1) >= (ROLE_LEVEL[props.minimum] ?? 999),
    props.children,
    props.fallback,
  );
}
