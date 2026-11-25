<script setup lang="ts">
import { computed } from 'vue'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import en from 'element-plus/es/locale/lang/en'

import Watermark from '@/components/Watermark.vue'
import { useI18n } from '@/i18n'

const { locale } = useI18n()
const elementLocale = computed(() => (locale.value === 'zh' ? zhCn : en))
</script>

<template>
  <el-config-provider :locale="elementLocale">
    <router-view />
    <Watermark />
  </el-config-provider>
</template>
