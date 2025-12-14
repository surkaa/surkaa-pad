<script setup lang="ts">
import {onMounted, ref} from "vue";
import {readCacheFile2UrlByEid, showToast} from "../utils";
import {useRouter} from "vue-router";

const router = useRouter();
const loading = ref(false);
const url = ref('');

onMounted(() => {
  // 从state获取临时文件路径
  const eid = history.state.eid;
  const minetype = history.state.minetype;
  if (eid && minetype) {
    loading.value = true;
    readCacheFile2UrlByEid(eid, minetype)
        .then(res => url.value = res)
        .catch(err => {
          console.error("Failed to load media:", err);
          showToast("图片加载失败", 'error', 3000, {
            position: "top-center"
          });
          router.back();
        })
        .finally(() => loading.value = false);
  }
});
</script>

<template>
  <div class="preview-media">
    <img alt="Preview" :src="url" v-click-outside="router.back"/>
  </div>
</template>

<style scoped lang="scss">
.preview-media {
  position: fixed;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 80vw;
  height: 80vh;

  display: flex;
  justify-content: center;
  align-items: center;
  flex-direction: column;

  img,
  video {
    max-width: 100%;
    max-height: 100%;
    display: block;
  }
}
</style>