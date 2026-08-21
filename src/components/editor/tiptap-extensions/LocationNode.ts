import {Node as TiptapNode, mergeAttributes} from '@tiptap/vue-3';
import type {Node as ProseMirrorNode} from '@tiptap/pm/model';
import type {NodeView} from '@tiptap/pm/view';
import type {DiaryLocation} from '../../../bindings';
import {formatLocationCoordinates} from '../../../utils/diaryLocation';

export interface LocationNodeOptions {
  onOpen: (location: DiaryLocation) => void;
}

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    locationNode: {
      insertLocation: (location: DiaryLocation) => ReturnType;
    };
  }
}

function parseLocation(value: unknown): DiaryLocation | null {
  if (!value || typeof value !== 'object') return null;
  const candidate = value as Partial<DiaryLocation>;
  if (candidate.coordinateSystem !== 'wgs84'
    || typeof candidate.latitude !== 'number'
    || typeof candidate.longitude !== 'number'
    || typeof candidate.capturedAt !== 'number') return null;
  return candidate as DiaryLocation;
}

function parseLocationAttribute(value?: string): DiaryLocation | null {
  if (!value) return null;
  try {
    return parseLocation(JSON.parse(value));
  } catch {
    return null;
  }
}

function accuracyText(location: DiaryLocation): string {
  const accuracy = location.horizontalAccuracyMeters;
  return typeof accuracy === 'number' && Number.isFinite(accuracy)
    ? `精度约 ±${Math.round(accuracy)} 米`
    : '精度未知';
}

function createLocationNodeView(
  initialNode: ProseMirrorNode,
  onOpen: LocationNodeOptions['onOpen'],
): NodeView {
  let currentLocation = parseLocation(initialNode.attrs.location);
  const dom = document.createElement('div');
  const icon = document.createElement('span');
  const body = document.createElement('div');
  const name = document.createElement('div');
  const details = document.createElement('div');
  const openButton = document.createElement('button');

  dom.className = 'editor-location';
  dom.contentEditable = 'false';
  icon.className = 'editor-location-icon';
  icon.textContent = '📍';
  body.className = 'editor-location-body';
  name.className = 'editor-location-name';
  details.className = 'editor-location-details';
  openButton.type = 'button';
  openButton.className = 'editor-location-open';
  openButton.title = '在地图中查看';
  openButton.setAttribute('aria-label', '在地图中查看');
  openButton.textContent = '↗';
  body.append(name, details);
  dom.append(icon, body, openButton);

  const sync = (node: ProseMirrorNode) => {
    const location = parseLocation(node.attrs.location);
    currentLocation = location;
    dom.dataset.location = JSON.stringify(location);
    name.textContent = location?.placeName?.trim() || '未命名地点';
    details.textContent = location
      ? `${formatLocationCoordinates(location)} · ${accuracyText(location)}`
      : '位置数据无效';
    openButton.disabled = !location;
  };
  sync(initialNode);

  const preventFocus = (event: Event) => {
    event.preventDefault();
    event.stopPropagation();
  };
  const handleOpen = (event: Event) => {
    preventFocus(event);
    if (currentLocation) onOpen(currentLocation);
  };
  openButton.addEventListener('pointerdown', preventFocus);
  openButton.addEventListener('click', handleOpen);

  return {
    dom,
    update(node) {
      if (node.type !== initialNode.type) return false;
      sync(node);
      return true;
    },
    stopEvent: event => openButton.contains(event.target as Node),
    ignoreMutation: () => true,
    destroy() {
      openButton.removeEventListener('pointerdown', preventFocus);
      openButton.removeEventListener('click', handleOpen);
    },
  };
}

export const LocationNode = TiptapNode.create<LocationNodeOptions>({
  name: 'locationNode',
  group: 'block',
  atom: true,
  selectable: true,
  draggable: true,

  addOptions() {
    return {onOpen: () => undefined};
  },

  addAttributes() {
    return {location: {default: null}};
  },

  parseHTML() {
    return [{
      tag: 'div.editor-location',
      getAttrs: element => ({
        location: parseLocationAttribute((element as HTMLElement).dataset.location),
      }),
    }];
  },

  renderHTML({node}) {
    const location = parseLocation(node.attrs.location);
    return [
      'div',
      mergeAttributes({
        class: 'editor-location',
        'data-location': JSON.stringify(location),
        contenteditable: 'false',
      }),
      ['span', {class: 'editor-location-icon'}, '📍'],
      ['div', {class: 'editor-location-body'},
        ['div', {class: 'editor-location-name'}, location?.placeName?.trim() || '未命名地点'],
        ['div', {class: 'editor-location-details'}, location
          ? `${formatLocationCoordinates(location)} · ${accuracyText(location)}`
          : '位置数据无效'],
      ],
      ['button', {type: 'button', class: 'editor-location-open'}, '↗'],
    ];
  },

  addNodeView() {
    return ({node}) => createLocationNodeView(node, this.options.onOpen);
  },

  addCommands() {
    return {
      insertLocation: location => ({commands}) => commands.insertContent({
        type: this.name,
        attrs: {location},
      }),
    };
  },
});
