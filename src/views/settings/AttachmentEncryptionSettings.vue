<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">附件加密</div>
    <q-list bordered separator class="pad-card">
      <q-item
        v-for="setting in encryptionSettings"
        :key="setting.key"
        tag="label"
        v-ripple
        class="settings-item"
      >
        <q-item-section avatar class="settings-icon-section">
          <q-icon :name="setting.icon"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">{{ setting.label }}</q-item-label>
          <q-item-label caption class="desc-text">{{ setting.description }}</q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-toggle v-model="setting.value.value" color="primary"/>
        </q-item-section>
      </q-item>
    </q-list>
  </section>
</template>

<script setup lang="ts">
import {useConfigStore} from '../../stores/config';

const configStore = useConfigStore();
const encryptionSettings = [
  {
    key: 'image',
    label: '图片',
    description: '控制新上传图片（含拍照）的加密状态',
    icon: 'image',
    value: configStore.useTauriConfig('encrypt_image_attachments'),
  },
  {
    key: 'audio',
    label: '音频',
    description: '控制新上传音频（含录音）的加密状态',
    icon: 'audiotrack',
    value: configStore.useTauriConfig('encrypt_audio_attachments'),
  },
  {
    key: 'video',
    label: '视频',
    description: '控制新上传视频的加密状态',
    icon: 'video_library',
    value: configStore.useTauriConfig('encrypt_video_attachments'),
  },
  {
    key: 'file',
    label: '文件',
    description: '控制其他新上传文件的加密状态',
    icon: 'attach_file',
    value: configStore.useTauriConfig('encrypt_file_attachments'),
  },
];
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
