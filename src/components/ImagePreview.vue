<template>
  <q-dialog
      no-refocus
      ref="dialogRef"
      @hide="onDialogHide"
      maximized
      transition-show="fade"
      transition-hide="fade"
  >
    <div class="flex flex-center" @click="onDialogCancel">

      <q-img
          :src="src"
          fit="contain"
          style="max-width: 95vw; max-height: 95vh;"
          @click.stop
      >
        <template v-slot:error>
          <div class="absolute-full flex flex-center bg-negative">
            图片加载失败
          </div>
        </template>
      </q-img>

      <q-btn
          flat
          round
          dense
          icon="close"
          size="lg"
          class="absolute-top-right q-ma-md"
          @click="onDialogCancel"
      />
    </div>
  </q-dialog>
</template>

<script setup lang="ts">
import {useDialogPluginComponent} from 'quasar'

// 接收外部传进来的 src
defineProps<{
  src: string
}>()

// 声明 Quasar Dialog 插件需要的 emits
defineEmits([
  ...useDialogPluginComponent.emits
])

// 获取 Dialog 插件的控制方法
const {dialogRef, onDialogHide, onDialogCancel} = useDialogPluginComponent()
</script>