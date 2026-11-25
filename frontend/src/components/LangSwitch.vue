<script setup lang="ts">
import { useI18n } from '@/i18n'

const { switchLabel, toggleLocale } = useI18n()
</script>

<template>
  <button class="lang-switch" type="button" :title="switchLabel" @click="toggleLocale">
    {{ switchLabel }}
  </button>
</template>

<style scoped>
.lang-switch {
  padding: 3px 10px;
  border: 1px solid var(--kst-border);
  border-radius: 4px;
  background: transparent;
  color: var(--kst-text-dim);
  font-size: 12px;
  line-height: 1.4;
  letter-spacing: 0.04em;
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s;
}

.lang-switch:hover {
  color: var(--kst-text);
  border-color: var(--kst-accent-strong);
}
</style>
