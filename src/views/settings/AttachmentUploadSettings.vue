<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">附件上传</div>
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
            dense
            outlined
            options-dense
            aria-label="同时上传数量"
            class="concurrency-select"
          />
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
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.concurrency-select {
  width: 76px;
}
</style>
