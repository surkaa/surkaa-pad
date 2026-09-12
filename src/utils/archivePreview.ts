import type {ArchivePreviewEntry} from '../bindings';

export interface ArchiveTreeNode extends ArchivePreviewEntry {
  key: string;
  label: string;
  depth: number;
  children: ArchiveTreeNode[];
  synthetic: boolean;
}

interface MutableArchiveTreeNode extends ArchiveTreeNode {
  childDirectories: Map<string, MutableArchiveTreeNode>;
}

export function buildArchiveTree(entries: ArchivePreviewEntry[]): ArchiveTreeNode[] {
  const root: MutableArchiveTreeNode[] = [];
  const directories = new Map<string, MutableArchiveTreeNode>();

  entries.forEach((entry, index) => {
    const segments = entry.path.split('/').filter(Boolean);
    if (!segments.length) return;
    let parentChildren = root;
    let currentPath = '';

    segments.slice(0, -1).forEach((segment, depth) => {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      const directory = ensureDirectory(
        directories,
        parentChildren,
        currentPath,
        segment,
        depth,
      );
      parentChildren = directory.children as MutableArchiveTreeNode[];
    });

    const label = segments[segments.length - 1]!;
    const depth = segments.length - 1;
    if (entry.isDirectory) {
      const directory = ensureDirectory(
        directories,
        parentChildren,
        entry.path,
        label,
        depth,
      );
      Object.assign(directory, entry, {synthetic: false});
      return;
    }
    parentChildren.push({
      ...entry,
      key: `file:${index}:${entry.path}`,
      label,
      depth,
      children: [],
      childDirectories: new Map(),
      synthetic: false,
    });
  });

  sortTree(root);
  return root.map(stripInternalFields);
}

export function initiallyExpandedDirectories(nodes: ArchiveTreeNode[]): Set<string> {
  return new Set(nodes.filter(node => node.isDirectory).map(node => node.key));
}

export function visibleArchiveNodes(
  nodes: ArchiveTreeNode[],
  expanded: ReadonlySet<string>,
  query: string,
): ArchiveTreeNode[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (normalizedQuery) return filterTree(nodes, normalizedQuery).flat;

  const visible: ArchiveTreeNode[] = [];
  const visit = (node: ArchiveTreeNode) => {
    visible.push(node);
    if (node.isDirectory && expanded.has(node.key)) node.children.forEach(visit);
  };
  nodes.forEach(visit);
  return visible;
}

function ensureDirectory(
  directories: Map<string, MutableArchiveTreeNode>,
  siblings: MutableArchiveTreeNode[],
  path: string,
  label: string,
  depth: number,
): MutableArchiveTreeNode {
  const existing = directories.get(path);
  if (existing) return existing;
  const directory: MutableArchiveTreeNode = {
    key: `dir:${path}`,
    path,
    label,
    depth,
    isDirectory: true,
    size: 0,
    compressedSize: 0,
    modifiedAt: null,
    encrypted: null,
    children: [],
    childDirectories: new Map(),
    synthetic: true,
  };
  directories.set(path, directory);
  siblings.push(directory);
  return directory;
}

function sortTree(nodes: MutableArchiveTreeNode[]) {
  nodes.sort((left, right) => {
    if (left.isDirectory !== right.isDirectory) return left.isDirectory ? -1 : 1;
    return left.label.localeCompare(right.label, undefined, {
      numeric: true,
      sensitivity: 'base',
    });
  });
  nodes.forEach(node => sortTree(node.children as MutableArchiveTreeNode[]));
}

function stripInternalFields(node: MutableArchiveTreeNode): ArchiveTreeNode {
  const {childDirectories: _childDirectories, ...result} = node;
  result.children = node.children.map(child => stripInternalFields(child as MutableArchiveTreeNode));
  return result;
}

function filterTree(
  nodes: ArchiveTreeNode[],
  query: string,
): {matched: boolean; flat: ArchiveTreeNode[]} {
  const flat: ArchiveTreeNode[] = [];
  let matched = false;
  nodes.forEach(node => {
    const childResult = filterTree(node.children, query);
    const nodeMatches = node.path.toLocaleLowerCase().includes(query);
    if (!nodeMatches && !childResult.matched) return;
    matched = true;
    flat.push(node, ...childResult.flat);
  });
  return {matched, flat};
}
