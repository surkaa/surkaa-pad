<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">附件设置</div>
    <q-list bordered separator class="pad-card">
      <q-item class="settings-item">
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="sync_alt"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">同时上传数量</q-item-label>
          <q-item-label caption class="desc-text">
            每批同时处理的附件数；较低更节省内存，范围 1–20
          </q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-select
            v-model="uploadConcurrency"
            :options="concurrencyOptions"
            :display-value="String(uploadConcurrency)"
            dense
            outlined
            options-dense
            popup-content-class="pad-upload-concurrency-menu"
            aria-label="同时上传数量"
            class="concurrency-select"
          />
        </q-item-section>
      </q-item>

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
import {
  MAX_UPLOAD_CONCURRENCY,
  MIN_UPLOAD_CONCURRENCY,
} from '../../utils/uploadConcurrency';

const configStore = useConfigStore();
const uploadConcurrency = configStore.useTauriConfig('attachment_upload_concurrency');
const concurrencyOptions = Array.from(
  {length: MAX_UPLOAD_CONCURRENCY - MIN_UPLOAD_CONCURRENCY + 1},
  (_, index) => index + MIN_UPLOAD_CONCURRENCY,
);
const encryptionSettings = [
  {
    key: 'image',
    label: '图片加密',
    description: '控制新上传图片（含拍照）的加密状态',
    icon: 'image',
    value: configStore.useTauriConfig('encrypt_image_attachments'),
  },
  {
    key: 'audio',
    label: '音频加密',
    description: '控制新上传音频（含录音）的加密状态',
    icon: 'audiotrack',
    value: configStore.useTauriConfig('encrypt_audio_attachments'),
  },
  {
    key: 'video',
    label: '视频加密',
    description: '控制新上传视频的加密状态',
    icon: 'video_library',
    value: configStore.useTauriConfig('encrypt_video_attachments'),
  },
  {
    key: 'file',
    label: '文件加密',
    description: '控制其他新上传文件的加密状态',
    icon: 'attach_file',
    value: configStore.useTauriConfig('encrypt_file_attachments'),
  },
];
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.concurrency-select {
  width: 76px;

  :deep(.q-field__native),
  :deep(.q-field__input) {
    color: var(--pad-text-color-200) !important;
  }

  :deep(.q-field__marginal) {
    color: var(--pad-text-color-300) !important;
  }
}
</style>
