<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const greetMsg = ref("");
const name = ref("");

async function testE2EChain() {
  const password = "0si3BxN9tLIq6Ych";
  // 实际项目中，你需要从 key_params.json 中读取 salt
  const saltBase64 = "aHR0cHM6Ly9nZW1pbmkuZ29vZ2xlLmNvbS9hcHAvMDU5MmNjODMwNzQ4MWQ0OA";

  // 1. 派生密钥 (KDF)
  const dek = await invoke<number[]>('derive_key', { password, salt: saltBase64 });
  console.log('DEK 派生成功:', dek.length === 32);

  // 2. 加密数据
  const plaintext = "这是我的秘密日记内容。时间: " + new Date().toISOString();
  const [ciphertext, iv] = await invoke<[number[], number[]]>('encrypt_data', {
    dek,
    plaintext
  });
  console.log(`加密成功。密文长度: ${ciphertext.length}, IV 长度: ${iv.length}`);

  // 3. 解密数据
  const decryptedText = await invoke<string>('decrypt_data', {
    dek,
    ciphertext,
    nonceBytes: iv,
  });

  // 4. 验证
  console.log('解密结果:', decryptedText);
  console.log('验证成功:', decryptedText === plaintext);

  // 5. 模拟数据篡改测试：
  const tamperedCiphertext = [...ciphertext];
  tamperedCiphertext[0] = tamperedCiphertext[0] + 1; // 改变密文的第一个字节
  try {
    await invoke<string>('decrypt_data', {
      dek,
      ciphertext: tamperedCiphertext,
      nonceBytes: iv,
    });
    console.error("篡改测试失败：篡改后的数据仍能解密！");
  } catch (e) {
    console.log("篡改测试成功：篡改后的数据解密失败（GCM Tag 验证失败）");
  }
}
</script>

<template>
  <main class="container">
    <h1>Welcome to Tauri + Vue</h1>

    <div class="row">
      <a href="https://vitejs.dev" target="_blank">
        <img src="/vite.svg" class="logo vite" alt="Vite logo" />
      </a>
      <a href="https://tauri.app" target="_blank">
        <img src="/tauri.svg" class="logo tauri" alt="Tauri logo" />
      </a>
      <a href="https://vuejs.org/" target="_blank">
        <img src="./assets/vue.svg" class="logo vue" alt="Vue logo" />
      </a>
    </div>
    <p>Click on the Tauri, Vite, and Vue logos to learn more.</p>

    <form class="row" @submit.prevent="testE2EChain">
      <input id="greet-input" v-model="name" placeholder="Enter a name..." />
      <button type="submit">Greet</button>
    </form>
    <p>{{ greetMsg }}</p>
  </main>
</template>

<style scoped>
.logo.vite:hover {
  filter: drop-shadow(0 0 2em #747bff);
}

.logo.vue:hover {
  filter: drop-shadow(0 0 2em #249b73);
}

</style>
<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
}

.logo {
  height: 6em;
  padding: 1.5em;
  will-change: filter;
  transition: 0.75s;
}

.logo.tauri:hover {
  filter: drop-shadow(0 0 2em #24c8db);
}

.row {
  display: flex;
  justify-content: center;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h1 {
  text-align: center;
}

input,
button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

button {
  cursor: pointer;
}

button:hover {
  border-color: #396cd8;
}
button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

input,
button {
  outline: none;
}

#greet-input {
  margin-right: 5px;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  a:hover {
    color: #24c8db;
  }

  input,
  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }
  button:active {
    background-color: #0f0f0f69;
  }
}

</style>