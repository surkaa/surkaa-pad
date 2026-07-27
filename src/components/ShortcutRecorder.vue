<script setup lang="ts">
import { ref } from 'vue'
import { formatEditorShortcut, shortcutFromKeyboardEvent } from '../utils/editorShortcuts'

const props = defineProps<{
  modelValue: string
  label: string
}>()

const emit = defineEmits<{
  (event: 'update:modelValue', value: string): void
  (event: 'invalid'): void
}>()

const recording = ref(false)

function handleKeydown(event: KeyboardEvent) {
  event.preventDefault()
  event.stopPropagation()

  if (event.code === 'Escape') {
    (event.currentTarget as HTMLElement | null)?.blur()
    return
  }
  if (
    (event.code === 'Backspace' || event.code === 'Delete')
    && !event.ctrlKey
    && !event.altKey
    && !event.shiftKey
  ) {
    emit('update:modelValue', '')
    return
  }

  const shortcut = shortcutFromKeyboardEvent(event)
  if (!shortcut) {
    if (!['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) emit('invalid')
    return
  }
  emit('update:modelValue', shortcut)
  const target = event.currentTarget as HTMLElement | null
  target?.blur()
}
</script>

<template>
  <q-input
    :model-value="recording ? '请按下快捷键…' : formatEditorShortcut(props.modelValue)"
    :label="label"
    outlined
    dense
    readonly
    class="shortcut-recorder"
    @focus="recording = true"
    @blur="recording = false"
    @keydown="handleKeydown"
  >
    <template #prepend>
      <q-icon name="keyboard"/>
    </template>
    <template #append>
      <q-btn
        v-if="modelValue"
        flat
        round
        dense
        icon="close"
        size="sm"
        aria-label="清除快捷键"
        @mousedown.prevent
        @click.stop="emit('update:modelValue', '')"
      />
    </template>
  </q-input>
</template>

<style scoped>
.shortcut-recorder {
  width: min(260px, 42vw);
}
</style>
