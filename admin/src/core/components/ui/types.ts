// admin/src/core/components/ui/types.ts
// ─────────────────────────────────────────────────────────────────────────────
// UI component prop interfaces. These live with the UI layer, not in core/types.ts.
// ─────────────────────────────────────────────────────────────────────────────

export interface PageState {
  limit: number;
  offset: number;
}

export interface SidebarProps {
  brand: { name: string; mark: string; subtitle: string };
  items?: NavItem[] | (() => NavItem[]);
  groups?: NavGroup[] | (() => NavGroup[]);
  footer: NavItem[];
  activeView: () => string;
  onNavigate: (view: string) => void;
  env?: string;
  context?: () => HTMLElement | null;
}

export interface NavItem {
  id: string;
  label: string;
  icon: string;  // SVG string
  danger?: boolean;
}

export interface NavGroup {
  label?: string;
  items: NavItem[] | (() => NavItem[]);
}

export interface HeaderProps {
  title: string;
  subtitle?: string;
  loading?: () => boolean;
  actions?: HTMLElement[];
}

export interface DataTableProps<T> {
  headers: string[];
  rows: () => T[];
  keyFn: (row: T) => string;
  renderRow: (row: T) => HTMLElement;
  emptyText?: string;
}

export interface PagerProps {
  total: () => number;
  page: () => PageState;
  onNavigate: (dir: 'prev' | 'next') => void;
}

export interface FilterBarProps {
  id: string;
  fields: FilterField[];
  onApply: () => void;
  onClear: () => void;
  onSave?: () => void;
  onLoad?: () => void;
  onExport?: () => void;
  total?: () => number;
}

export interface FilterField {
  name: string;
  label: string;
  type: 'text' | 'select' | 'datetime';
  value: () => string;
  onChange: (value: string) => void;
  placeholder?: string;
  options?: () => { value: string; label: string }[];
}
