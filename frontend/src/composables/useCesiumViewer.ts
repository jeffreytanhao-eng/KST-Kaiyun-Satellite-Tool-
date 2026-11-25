import { onBeforeUnmount, ref, shallowRef, type Ref } from 'vue'
import * as Cesium from 'cesium'

import type { ScenarioData, SatelliteObject } from '@/api/scenarios'
import type { AccessReport, AccessWindow, SatelliteTrack, TrackSample } from '@/api/jobs'

const ACCESS_COLOR = '#ffb74d'

interface SatelliteVisual {
  data: SatelliteObject
  track: SatelliteTrack | null
  entity: Cesium.Entity
}

interface StationPosition {
  longitude: number
  latitude: number
  altitudeKm: number
}

/**
 * Cesium 视口与多对象渲染：离线影像、卫星轨迹、地面站、回放位置与 Access 高亮。
 * 所有轨道数据均由后端计算，这里只做可视化。
 */
export function useCesiumViewer(host: Ref<HTMLDivElement | null>) {
  const viewer = shallowRef<Cesium.Viewer | null>(null)
  const ready = ref(false)

  const satellites = new Map<string, SatelliteVisual>()
  const stations = new Map<string, StationPosition>()
  const stationEntities = new Map<string, Cesium.Entity>()

  let accessWindows: AccessWindow[] = []
  let accessSatelliteId: string | null = null
  let accessStationId: string | null = null
  let accessEntities: Cesium.Entity[] = []
  let linkEntity: Cesium.Entity | null = null
  let playhead = 0
  async function init() {
    if (!host.value) return

    const instance = new Cesium.Viewer(host.value, {
      baseLayer: false,
      animation: false,
      timeline: false,
      baseLayerPicker: false,
      fullscreenButton: false,
      geocoder: false,
      homeButton: false,
      infoBox: false,
      navigationHelpButton: false,
      sceneModePicker: false,
      selectionIndicator: false,
      creditContainer: document.createElement('div')
    })

    const scene = instance.scene
    scene.backgroundColor = Cesium.Color.fromCssColorString('#04060b')
    scene.globe.baseColor = Cesium.Color.fromCssColorString('#0b1526')
    scene.globe.enableLighting = false
    scene.globe.showGroundAtmosphere = true
    scene.globe.maximumScreenSpaceError = 2
    scene.globe.depthTestAgainstTerrain = false
    if (scene.skyAtmosphere) scene.skyAtmosphere.show = true
    scene.screenSpaceCameraController.enableCollisionDetection = true
    scene.screenSpaceCameraController.minimumZoomDistance = 500_000
    scene.screenSpaceCameraController.maximumZoomDistance = 400 * 6_378_137

    viewer.value = instance
    instance.camera.setView({
      destination: Cesium.Cartesian3.fromDegrees(110, 20, 20_000_000)
    })

    await addOfflineImagery(instance)
    ready.value = true
  }

  async function addOfflineImagery(instance: Cesium.Viewer) {
    const localTiles = `${import.meta.env.BASE_URL}tiles/earth`
    try {
      const provider = await Cesium.TileMapServiceImageryProvider.fromUrl(localTiles, {
        credit: ''
      })
      instance.imageryLayers.addImageryProvider(provider)
      return
    } catch (error) {
      console.warn('[kst] 本地离线影像加载失败，回退到 NaturalEarthII', error)
    }

    try {
      const provider = await Cesium.TileMapServiceImageryProvider.fromUrl(
        Cesium.buildModuleUrl('Assets/Textures/NaturalEarthII'),
        { credit: '' }
      )
      instance.imageryLayers.addImageryProvider(provider)
    } catch (error) {
      console.warn('[kst] 内置离线影像不可用', error)
    }
  }

  function clearScene() {
    const instance = viewer.value
    if (!instance) return
    instance.entities.removeAll()
    satellites.clear()
    stations.clear()
    stationEntities.clear()
    accessWindows = []
    accessSatelliteId = null
    accessStationId = null
    accessEntities = []
    linkEntity = null
    playhead = 0
  }

  function renderScenario(data: ScenarioData, tracks: SatelliteTrack[] | null) {
    const instance = viewer.value
    if (!instance) return

    clearScene()

    for (const station of data.ground_stations) {
      const color = Cesium.Color.fromCssColorString(station.color || '#ffd54f')
      const position = Cesium.Cartesian3.fromDegrees(
        station.longitude_deg,
        station.latitude_deg,
        station.altitude_km * 1000
      )
      const entity = instance.entities.add({
        id: `gs-${station.id}`,
        position,
        show: station.enabled,
        point: {
          pixelSize: 9,
          color: color.withAlpha(0.95),
          outlineColor: Cesium.Color.fromCssColorString('#04060b'),
          outlineWidth: 2,
          disableDepthTestDistance: Number.POSITIVE_INFINITY
        },
        label: {
          text: station.name,
          font: '12px "Microsoft YaHei", "Segoe UI", sans-serif',
          fillColor: Cesium.Color.WHITE,
          showBackground: true,
          backgroundColor: Cesium.Color.fromCssColorString('rgba(6, 12, 22, 0.78)'),
          backgroundPadding: new Cesium.Cartesian2(7, 3),
          pixelOffset: new Cesium.Cartesian2(0, 18),
          disableDepthTestDistance: Number.POSITIVE_INFINITY
        }
      })
      stationEntities.set(station.id, entity)
      stations.set(station.id, {
        longitude: station.longitude_deg,
        latitude: station.latitude_deg,
        altitudeKm: station.altitude_km
      })
    }

    const trackMap = new Map<string, SatelliteTrack>()
    for (const track of tracks ?? []) {
      trackMap.set(track.satellite_id, track)
    }

    data.satellites.forEach((satellite, index) => {
      const track = trackMap.get(satellite.id) ?? null
      const color = Cesium.Color.fromCssColorString(satellite.color || fallbackColor(index))

      if (track && track.samples.length > 1) {
        instance.entities.add({
          id: `orbit-${satellite.id}`,
          polyline: {
            positions: Cesium.Cartesian3.fromDegreesArrayHeights(
              track.samples.flatMap((sample) => [
                sample.longitude_deg,
                sample.latitude_deg,
                sample.altitude_km * 1000
              ])
            ),
            width: 2.5,
            arcType: Cesium.ArcType.NONE,
            material: new Cesium.PolylineGlowMaterialProperty({
              glowPower: 0.15,
              taperPower: 1.0,
              color: color.withAlpha(0.85)
            })
          }
        })
      }

      const initial = track && track.samples.length > 0 ? track.samples[0] : null
      const position = initial
        ? Cesium.Cartesian3.fromDegrees(
            initial.longitude_deg,
            initial.latitude_deg,
            initial.altitude_km * 1000
          )
        : Cesium.Cartesian3.fromDegrees(0, 0, 0)

      const entity = instance.entities.add({
        id: `sat-${satellite.id}`,
        position,
        show: satellite.enabled && Boolean(track),
        point: {
          pixelSize: 9,
          color,
          outlineColor: Cesium.Color.WHITE.withAlpha(0.9),
          outlineWidth: 1.5,
          disableDepthTestDistance: Number.POSITIVE_INFINITY
        },
        label: {
          text: satellite.name,
          font: '13px "Microsoft YaHei", "Segoe UI", sans-serif',
          fillColor: Cesium.Color.WHITE,
          showBackground: true,
          backgroundColor: Cesium.Color.fromCssColorString('rgba(6, 12, 22, 0.78)'),
          backgroundPadding: new Cesium.Cartesian2(8, 3),
          pixelOffset: new Cesium.Cartesian2(0, -22),
          disableDepthTestDistance: Number.POSITIVE_INFINITY
        }
      })

      satellites.set(satellite.id, { data: satellite, track, entity })
    })

    playhead = 0
    setPlayhead(0)
  }

  function setPlayhead(tSeconds: number) {
    playhead = tSeconds
    const instance = viewer.value
    if (!instance) return

    for (const visual of satellites.values()) {
      if (!visual.track) continue
      const sample = sampleAt(visual.track.samples, tSeconds)
      if (!sample) continue
      visual.entity.position = new Cesium.ConstantPositionProperty(
        Cesium.Cartesian3.fromDegrees(
          sample.longitude_deg,
          sample.latitude_deg,
          sample.altitude_km * 1000
        )
      )
    }

    updateAccessLink()
  }

  function updateAccessLink() {
    const instance = viewer.value
    if (!instance) return

    const within = accessWindows.find(
      (window) => playhead >= window.start_seconds && playhead <= window.end_seconds
    )
    if (!within || !accessSatelliteId || !accessStationId) {
      if (linkEntity) linkEntity.show = false
      return
    }

    const visual = satellites.get(accessSatelliteId)
    const station = stations.get(accessStationId)
    if (!visual?.track || !station) {
      if (linkEntity) linkEntity.show = false
      return
    }
    const sample = sampleAt(visual.track.samples, playhead)
    if (!sample) {
      if (linkEntity) linkEntity.show = false
      return
    }

    const positions = [
      Cesium.Cartesian3.fromDegrees(
        station.longitude,
        station.latitude,
        station.altitudeKm * 1000
      ),
      Cesium.Cartesian3.fromDegrees(
        sample.longitude_deg,
        sample.latitude_deg,
        sample.altitude_km * 1000
      )
    ]

    if (linkEntity) {
      linkEntity.polyline!.positions = new Cesium.ConstantProperty(positions)
      linkEntity.show = true
    } else {
      linkEntity = instance.entities.add({
        id: 'access-link',
        polyline: {
          positions,
          width: 2,
          arcType: Cesium.ArcType.NONE,
          material: new Cesium.PolylineDashMaterialProperty({
            color: Cesium.Color.fromCssColorString(ACCESS_COLOR).withAlpha(0.9),
            dashLength: 12
          })
        }
      })
    }
  }

  function highlightAccess(report: AccessReport | null, timeOffsetSeconds = 0) {
    const instance = viewer.value
    if (!instance) return

    for (const entity of accessEntities) {
      instance.entities.remove(entity)
    }
    accessEntities = []
    accessWindows = (report?.windows ?? []).map((window) => ({
      ...window,
      start_seconds: window.start_seconds + timeOffsetSeconds,
      end_seconds: window.end_seconds + timeOffsetSeconds
    }))
    accessSatelliteId = report?.satellite_id ?? null
    accessStationId = report?.ground_station_id ?? null

    if (report && accessWindows.length > 0) {
      const visual = satellites.get(report.satellite_id)
      if (visual?.track) {
        const color = Cesium.Color.fromCssColorString(ACCESS_COLOR)
        for (const window of accessWindows) {
          const points = visual.track.samples.filter(
            (sample) =>
              sample.t_seconds >= window.start_seconds &&
              sample.t_seconds <= window.end_seconds
          )
          if (points.length < 2) continue
          const arc = instance.entities.add({
            id: `access-arc-${window.index}`,
            polyline: {
              positions: Cesium.Cartesian3.fromDegreesArrayHeights(
                points.flatMap((sample) => [
                  sample.longitude_deg,
                  sample.latitude_deg,
                  sample.altitude_km * 1000
                ])
              ),
              width: 5,
              arcType: Cesium.ArcType.NONE,
              material: new Cesium.PolylineGlowMaterialProperty({
                glowPower: 0.3,
                color: color.withAlpha(0.95)
              })
            }
          })
          accessEntities.push(arc)
        }
      }
    }

    updateAccessLink()
  }

  function flyToScenario() {
    const instance = viewer.value
    if (!instance) return

    for (const visual of satellites.values()) {
      if (!visual.track || visual.track.samples.length === 0) continue
      const sample = visual.track.samples[0]
      instance.camera.flyTo({
        destination: Cesium.Cartesian3.fromDegrees(
          sample.longitude_deg,
          Math.max(-75, Math.min(75, sample.latitude_deg - 18)),
          17_000_000
        ),
        orientation: {
          heading: 0,
          pitch: Cesium.Math.toRadians(-72),
          roll: 0
        },
        duration: 1.2
      })
      return
    }

    const station = stations.values().next().value as StationPosition | undefined
    instance.camera.flyTo({
      destination: Cesium.Cartesian3.fromDegrees(
        station?.longitude ?? 110,
        station?.latitude ?? 20,
        17_000_000
      ),
      orientation: { heading: 0, pitch: Cesium.Math.toRadians(-72), roll: 0 },
      duration: 1.2
    })
  }

  function flyToLocation(longitude: number, latitude: number, altitudeKm = 0) {
    const instance = viewer.value
    if (!instance) return
    instance.camera.flyTo({
      destination: Cesium.Cartesian3.fromDegrees(
        longitude,
        latitude,
        Math.max(altitudeKm * 1000, 600_000)
      ),
      orientation: { heading: 0, pitch: Cesium.Math.toRadians(-70), roll: 0 },
      duration: 1.0
    })
  }

  function destroy() {
    if (viewer.value && !viewer.value.isDestroyed()) {
      viewer.value.destroy()
    }
    viewer.value = null
    ready.value = false
  }

  onBeforeUnmount(destroy)

  return {
    viewer,
    ready,
    init,
    renderScenario,
    setPlayhead,
    highlightAccess,
    flyToScenario,
    flyToLocation,
    destroy
  }
}

function fallbackColor(index: number) {
  const palette = ['#4fc3f7', '#ffb74d', '#81c784', '#e57373', '#ba68c8', '#4db6ac']
  return palette[index % palette.length]
}

/** 按时间线性插值轨迹采样点。 */
function sampleAt(samples: TrackSample[], tSeconds: number): TrackSample | null {
  if (samples.length === 0) return null
  const first = samples[0]
  const last = samples[samples.length - 1]
  if (tSeconds <= first.t_seconds) return first
  if (tSeconds >= last.t_seconds) return last

  const step = samples.length > 1 ? samples[1].t_seconds - first.t_seconds : 1
  const index = Math.min(
    Math.max(Math.floor((tSeconds - first.t_seconds) / (step || 1)), 0),
    samples.length - 2
  )
  const a = samples[index]
  const b = samples[index + 1]
  const span = b.t_seconds - a.t_seconds || 1
  const fraction = (tSeconds - a.t_seconds) / span

  return {
    t_seconds: tSeconds,
    longitude_deg: a.longitude_deg + (b.longitude_deg - a.longitude_deg) * fraction,
    latitude_deg: a.latitude_deg + (b.latitude_deg - a.latitude_deg) * fraction,
    altitude_km: a.altitude_km + (b.altitude_km - a.altitude_km) * fraction
  }
}
