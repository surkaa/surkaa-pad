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
const inputRef = ref<{ focus: () => void; blur: () => void } | null>(null)

function startRecording() {
  recording.value = true
  inputRef.value?.focus()
}

function stopRecording() {
  recording.value = false
  inputRef.value?.blur()
}

function handleKeydown(event: KeyboardEvent) {
  event.preventDefault()
  event.stopPropagation()

  if (event.code === 'Escape') {
    stopRecording()
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
  stopRecording()
}
</script>

<template>
  <q-input
    ref="inputRef"
    :model-value="recording ? '正在录制：请按组合键' : formatEditorShortcut(props.modelValue)"
    :label="recording ? '正在录制' : label"
    :color="recording ? 'primary' : undefined"
    :class="{ 'is-recording': recording }"
    :aria-label="recording ? `${label}快捷键正在录制` : `${label}快捷键`"
    outlined
    dense
    readonly
    class="shortcut-recorder"
    @click="startRecording"
    @focusin="recording = true"
    @focusout="recording = false"
    @keydown="handleKeydown"
  >
    <template #prepend>
      <q-icon
        :name="recording ? 'radio_button_checked' : 'keyboard'"
        :color="recording ? 'primary' : undefined"
        :class="{ 'recording-indicator': recording }"
      />
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

.shortcut-recorder :deep(.q-field__native) {
  cursor: pointer;
}

.shortcut-recorder.is-recording :deep(.q-field__control) {
  box-shadow: 0 0 0 2px var(--q-primary);
}

.shortcut-recorder.is-recording :deep(.q-field__native),
.shortcut-recorder.is-recording :deep(.q-field__label) {
  color: var(--pad-primary-color) !important;
}

.recording-indicator {
  animation: recording-pulse 1s ease-in-out infinite;
}

@keyframes recording-pulse {
  50% {
    opacity: 0.45;
    transform: scale(0.85);
  }
}
</style>
