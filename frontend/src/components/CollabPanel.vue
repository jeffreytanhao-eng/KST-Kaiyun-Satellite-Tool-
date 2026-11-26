<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'

import {
  addComment,
  addShare,
  listComments,
  listShares,
  removeShare,
  updateShare,
  type ScenarioComment,
  type ScenarioShare,
  type SharePermission
} from '@/api/collaboration'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const props = defineProps<{ active?: boolean }>()

const store = useScenarioStore()
const { t } = useI18n()

const shares = ref<ScenarioShare[]>([])
const comments = ref<ScenarioComment[]>([])
const shareUsername = ref('')
const sharePermission = ref<SharePermission>('read')
const commentBody = ref('')
const loading = ref(false)
const busy = ref(false)

const scenarioId = computed(() => store.current?.id ?? '')
const canManage = computed(
  () => store.current?.permission === 'owner' || store.current?.permission === 'admin'
)

async function loadShares() {
  if (!scenarioId.value || !canManage.value) {
    shares.value = []
    return
  }
  try {
    shares.value = await listShares(scenarioId.value)
  } catch (error) {
    ElMessage.error(`${t('shareFailed')}: ${errorText(error)}`)
  }
}

async function loadComments() {
  if (!scenarioId.value) {
    comments.value = []
    return
  }
  try {
    comments.value = await listComments(scenarioId.value)
  } catch (error) {
    ElMessage.error(`${t('commentFailed')}: ${errorText(error)}`)
  }
}

async function loadAll() {
  loading.value = true
  try {
    await Promise.all([loadShares(), loadComments()])
  } finally {
    loading.value = false
  }
}

async function onAddShare() {
  const username = shareUsername.value.trim()
  if (!username) {
    ElMessage.warning(t('shareNeedUser'))
    return
  }
  busy.value = true
  try {
    await addShare(scenarioId.value, username, sharePermission.value)
    shareUsername.value = ''
    await loadShares()
    ElMessage.success(t('shareAdded'))
  } catch (error) {
    ElMessage.error(`${t('shareFailed')}: ${errorText(error)}`)
  } finally {
    busy.value = false
  }
}

async function onUpdateShare(share: ScenarioShare, permission: SharePermission) {
  try {
    const updated = await updateShare(scenarioId.value, share.user_id, permission)
    const index = shares.value.findIndex((item) => item.id === share.id)
    if (index >= 0) shares.value[index] = updated
    ElMessage.success(t('shareAdded'))
  } catch (error) {
    ElMessage.error(`${t('shareFailed')}: ${errorText(error)}`)
  }
}

async function onRemoveShare(share: ScenarioShare) {
  try {
    await removeShare(scenarioId.value, share.user_id)
    shares.value = shares.value.filter((item) => item.id !== share.id)
    ElMessage.success(t('shareRemoved'))
  } catch (error) {
    ElMessage.error(`${t('shareFailed')}: ${errorText(error)}`)
  }
}

async function onAddComment() {
  const body = commentBody.value.trim()
  if (!body) return
  busy.value = true
  try {
    const created = await addComment(scenarioId.value, body)
    comments.value.push(created)
    commentBody.value = ''
  } catch (error) {
    ElMessage.error(`${t('commentFailed')}: ${errorText(error)}`)
  } finally {
    busy.value = false
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

onMounted(loadAll)

watch(() => props.active, (active) => {
  if (active) void loadAll()
})

watch(scenarioId, () => {
  void loadAll()
})
</script>

<template>
  <div class="section-title collab-title">
    <span>{{ t('collabTitle') }}</span>
    <el-button link size="small" :loading="loading" @click="loadAll">
      {{ t('refresh') }}
    </el-button>
  </div>

  <template v-if="store.current">
    <div class="section-title">{{ t('shareTitle') }}</div>
    <div v-if="!canManage" class="hint">{{ t('shareOwnerOnly') }}</div>
    <template v-else>
      <div class="share-form">
        <el-input
          v-model="shareUsername"
          size="small"
          :placeholder="t('shareUserPlaceholder')"
          @keyup.enter="onAddShare"
        />
        <el-select v-model="sharePermission" size="small" style="width: 96px">
          <el-option :label="t('shareRead')" value="read" />
          <el-option :label="t('shareWrite')" value="write" />
        </el-select>
        <el-button size="small" type="primary" :loading="busy" @click="onAddShare">
          {{ t('shareAdd') }}
        </el-button>
      </div>

      <div v-if="shares.length === 0" class="hint">{{ t('shareEmpty') }}</div>
      <div v-else class="share-list">
        <div v-for="share in shares" :key="share.id" class="share-item">
          <span class="share-name">{{ share.username }}</span>
          <el-select
            :model-value="share.permission"
            size="small"
            style="width: 92px"
            @update:model-value="(value: SharePermission) => onUpdateShare(share, value)"
          >
            <el-option :label="t('shareRead')" value="read" />
            <el-option :label="t('shareWrite')" value="write" />
          </el-select>
          <el-button size="small" type="danger" plain @click="onRemoveShare(share)">
            {{ t('shareRemove') }}
          </el-button>
        </div>
      </div>
    </template>

    <div class="section-title">{{ t('commentTitle') }}</div>
    <el-input
      v-model="commentBody"
      type="textarea"
      :rows="3"
      maxlength="2000"
      show-word-limit
      :placeholder="t('commentPlaceholder')"
    />
    <el-button
      size="small"
      type="primary"
      class="full"
      :loading="busy"
      :disabled="!commentBody.trim()"
      @click="onAddComment"
    >
      {{ t('commentAdd') }}
    </el-button>

    <div v-if="comments.length === 0" class="hint">{{ t('commentEmpty') }}</div>
    <div v-else class="comment-list">
      <div v-for="comment in comments" :key="comment.id" class="comment-item">
        <div class="comment-head">
          <span class="comment-author">{{ comment.username || t('commentUnknown') }}</span>
          <span class="comment-time">{{ new Date(comment.created_at).toLocaleString() }}</span>
        </div>
        <div class="comment-body">{{ comment.body }}</div>
      </div>
    </div>
  </template>
  <div v-else class="hint">{{ t('noScenarioSelected') }}</div>
</template>

<style scoped lang="scss">
.section-title {
  margin: 12px 0 8px;
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent-strong);
}

.collab-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 0;
}

.hint {
  margin-top: 6px;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--kst-text-dim);
}

.share-form {
  display: flex;
  gap: 6px;
  align-items: center;
}

.share-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.share-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.55);
}

.share-name {
  flex: 1;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.full {
  width: 100%;
  margin-top: 8px;
}

.comment-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.comment-item {
  padding: 7px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.55);
}

.comment-head {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 10px;
  color: var(--kst-text-dim);
}

.comment-author {
  font-weight: 600;
  color: var(--kst-text);
}

.comment-body {
  margin-top: 4px;
  font-size: 10.5px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
