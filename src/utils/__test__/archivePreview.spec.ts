import {describe, expect, it} from 'vitest';
import type {ArchivePreviewEntry} from '../../bindings';
import {
  buildArchiveTree,
  initiallyExpandedDirectories,
  visibleArchiveNodes,
} from '../archivePreview';

function entry(
  path: string,
  isDirectory = false,
  size = 0,
): ArchivePreviewEntry {
  return {
    path,
    isDirectory,
    size,
    compressedSize: size,
    modifiedAt: null,
    encrypted: null,
  };
}

describe('buildArchiveTree', () => {
  it('creates implicit directories and sorts directories before files naturally', () => {
    const tree = buildArchiveTree([
      entry('file10.txt', false, 10),
      entry('folder/file2.txt', false, 2),
      entry('file2.txt', false, 2),
      entry('folder/nested/data.json', false, 3),
    ]);

    expect(tree.map(node => node.label)).toEqual(['folder', 'file2.txt', 'file10.txt']);
    expect(tree[0]).toMatchObject({path: 'folder', isDirectory: true, synthetic: true});
    expect(tree[0]?.children.map(node => node.label)).toEqual(['nested', 'file2.txt']);
    expect(tree[0]?.children[0]?.children[0]).toMatchObject({path: 'folder/nested/data.json'});
  });

  it('uses explicit directory metadata when the archive contains it', () => {
    const tree = buildArchiveTree([
      entry('folder/file.txt', false, 4),
      {...entry('folder', true), modifiedAt: '2026-09-12 12:00:00'},
    ]);

    expect(tree[0]).toMatchObject({
      path: 'folder',
      isDirectory: true,
      synthetic: false,
      modifiedAt: '2026-09-12 12:00:00',
    });
    expect(tree[0]?.children).toHaveLength(1);
  });
});

describe('visibleArchiveNodes', () => {
  const tree = buildArchiveTree([
    entry('docs/Guide.md', false, 4),
    entry('docs/internal/design.md', false, 5),
    entry('photo.jpg', false, 6),
  ]);

  it('only includes descendants of expanded directories', () => {
    const expanded = initiallyExpandedDirectories(tree);
    expect(visibleArchiveNodes(tree, expanded, '').map(node => node.path)).toEqual([
      'docs',
      'docs/internal',
      'docs/Guide.md',
      'photo.jpg',
    ]);

    expect(visibleArchiveNodes(tree, new Set(), '').map(node => node.path)).toEqual([
      'docs',
      'photo.jpg',
    ]);
  });

  it('matches paths case-insensitively and keeps their ancestors visible', () => {
    expect(visibleArchiveNodes(tree, new Set(), 'DESIGN').map(node => node.path)).toEqual([
      'docs',
      'docs/internal',
      'docs/internal/design.md',
    ]);
  });
});
