<script setup lang="ts">
import { Delete, Plus } from '@element-plus/icons-vue'

import type { GroundStationObject, SatelliteObject, SensorObject } from '@/api/scenarios'
import { useI18n } from '@/i18n'

type ObjectType = 'satellite' | 'station' | 'sensor'

interface ObjectSelection {
  type: ObjectType
  id: string
}

defineProps<{
  satellites: SatelliteObject[]
  stations: GroundStationObject[]
  sensors: SensorObject[]
  selected: ObjectSelection | null
}>()

const emit = defineEmits<{
  (event: 'select', payload: ObjectSelection): void
  (event: 'add', type: ObjectType): void
  (event: 'delete', payload: ObjectSelection): void
  (event: 'toggle', payload: ObjectSelection & { enabled: boolean }): void
}>()

const { t } = useI18n()

function isActive(
  selected: ObjectSelection | null,
  type: ObjectType,
  id: string
) {
  return selected?.type === type && selected.id === id
}
</script>

<template>
  <div class="tree">
    <section class="group">
      <header class="group-head">
        <span class="group-title">{{ t('satellites') }} · {{ satellites.length }}</span>
        <el-button link size="small" class="add" :title="t('addSatellite')" @click="emit('add', 'satellite')">
          <el-icon><Plus /></el-icon>
        </el-button>
      </header>
      <div v-if="satellites.length === 0" class="empty">{{ t('noSatellites') }}</div>
      <div
        v-for="satellite in satellites"
        :key="satellite.id"
        class="item"
        :class="{ active: isActive(selected, 'satellite', satellite.id) }"
        @click="emit('select', { type: 'satellite', id: satellite.id })"
      >
        <span class="dot" :style="{ background: satellite.color || '#4fc3f7' }" />
        <span class="name" :title="satellite.name">{{ satellite.name }}</span>
        <el-switch
          size="small"
          :model-value="satellite.enabled"
          class="toggle"
          @click.stop
          @update:model-value="
            (value: string | number | boolean) =>
              emit('toggle', { type: 'satellite', id: satellite.id, enabled: Boolean(value) })
          "
        />
        <el-button
          link
          size="small"
          class="remove"
          :title="t('delete')"
          @click.stop="emit('delete', { type: 'satellite', id: satellite.id })"
        >
          <el-icon><Delete /></el-icon>
        </el-button>
      </div>
    </section>

    <section class="group">
      <header class="group-head">
        <span class="group-title">{{ t('groundStations') }} · {{ stations.length }}</span>
        <el-button link size="small" class="add" :title="t('addStation')" @click="emit('add', 'station')">
          <el-icon><Plus /></el-icon>
        </el-button>
      </header>
      <div v-if="stations.length === 0" class="empty">{{ t('noStations') }}</div>
      <div
        v-for="station in stations"
        :key="station.id"
        class="item"
        :class="{ active: isActive(selected, 'station', station.id) }"
        @click="emit('select', { type: 'station', id: station.id })"
      >
        <span class="dot square" :style="{ background: station.color || '#ffd54f' }" />
        <span class="name" :title="station.name">{{ station.name }}</span>
        <el-switch
          size="small"
          :model-value="station.enabled"
          class="toggle"
          @click.stop
          @update:model-value="
            (value: string | number | boolean) =>
              emit('toggle', { type: 'station', id: station.id, enabled: Boolean(value) })
          "
        />
        <el-button
          link
          size="small"
          class="remove"
          :title="t('delete')"
          @click.stop="emit('delete', { type: 'station', id: station.id })"
        >
          <el-icon><Delete /></el-icon>
        </el-button>
      </div>
    </section>

    <section class="group">
      <header class="group-head">
        <span class="group-title">{{ t('sensors') }} · {{ sensors.length }}</span>
        <el-button link size="small" class="add" :title="t('addSensor')" @click="emit('add', 'sensor')">
          <el-icon><Plus /></el-icon>
        </el-button>
      </header>
      <div v-if="sensors.length === 0" class="empty">{{ t('noSensors') }}</div>
      <div
        v-for="sensor in sensors"
        :key="sensor.id"
        class="item"
        :class="{ active: isActive(selected, 'sensor', sensor.id) }"
        @click="emit('select', { type: 'sensor', id: sensor.id })"
      >
        <span class="dot triangle" :style="{ background: sensor.color || '#b39ddb' }" />
        <span class="name" :title="sensor.name">{{ sensor.name }}</span>
        <el-switch
          size="small"
          :model-value="sensor.enabled"
          class="toggle"
          @click.stop
          @update:model-value="
            (value: string | number | boolean) =>
              emit('toggle', { type: 'sensor', id: sensor.id, enabled: Boolean(value) })
          "
        />
        <el-button
          link
          size="small"
          class="remove"
          :title="t('delete')"
          @click.stop="emit('delete', { type: 'sensor', id: sensor.id })"
        >
          <el-icon><Delete /></el-icon>
        </el-button>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.tree {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.group {
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  overflow: hidden;
}

.group-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  background: rgba(79, 195, 247, 0.06);
  border-bottom: 1px solid var(--kst-border);
}

.group-title {
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent-strong);
}

.add {
  color: var(--kst-text-dim);

  &:hover {
    color: var(--kst-accent-strong);
  }
}

.empty {
  padding: 10px;
  font-size: 11px;
  color: var(--kst-text-dim);
}

.item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  cursor: pointer;
  border-bottom: 1px solid rgba(38, 55, 78, 0.5);
  transition: background 0.15s ease;

  &:last-child {
    border-bottom: none;
  }

  &:hover {
    background: rgba(79, 195, 247, 0.08);
  }

  &.active {
    background: rgba(79, 195, 247, 0.16);
    box-shadow: inset 2px 0 0 var(--kst-accent-strong);
  }
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
  box-shadow: 0 0 6px currentColor;
}

.dot.square {
  border-radius: 2px;
}

.dot.triangle {
  border-radius: 2px;
  transform: rotate(45deg);
  box-shadow: none;
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--kst-text);
}

.toggle {
  flex: none;
  --el-switch-on-color: var(--kst-accent-strong);
  transform: scale(0.82);
}

.remove {
  flex: none;
  color: var(--kst-text-dim);
  padding: 0;

  &:hover {
    color: #e57373;
  }
}
</style>
