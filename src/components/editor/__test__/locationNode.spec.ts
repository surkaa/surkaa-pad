// @vitest-environment happy-dom
import {describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {LocationNode} from '../tiptap-extensions/LocationNode';
import type {DiaryLocation} from '../../../bindings';

const location: DiaryLocation = {
  coordinateSystem: 'wgs84',
  latitude: 23.1291,
  longitude: 113.2644,
  horizontalAccuracyMeters: 18.5,
  capturedAt: 1_787_392_800_000,
  placeName: '广州市越秀区',
  altitudeMeters: null,
  verticalAccuracyMeters: null,
};

describe('LocationNode', () => {
  it('inserts and renders a structured location card', () => {
    const element = document.createElement('div');
    document.body.appendChild(element);
    const editor = new Editor({
      element,
      extensions: [StarterKit, LocationNode],
      content: '<p></p>',
    });

    expect(editor.commands.insertLocation(location)).toBe(true);
    expect(editor.getJSON().content).toContainEqual(expect.objectContaining({
      type: 'locationNode',
      attrs: {location},
    }));
    expect(element.querySelector('.editor-location-name')?.textContent).toBe('广州市越秀区');
    expect(element.querySelector('.editor-location-details')?.textContent)
      .toContain('23.129100, 113.264400');

    editor.destroy();
    element.remove();
  });

  it('opens the stored location without focusing editable content', () => {
    const element = document.createElement('div');
    document.body.appendChild(element);
    const onOpen = vi.fn();
    const editor = new Editor({
      element,
      extensions: [StarterKit, LocationNode.configure({onOpen})],
      content: `<div class="editor-location" data-location="${JSON.stringify(location).replace(/"/g, '&quot;')}"></div>`,
    });

    (element.querySelector('.editor-location-open') as HTMLButtonElement).click();
    expect(onOpen).toHaveBeenCalledWith(location);

    editor.destroy();
    element.remove();
  });
});
