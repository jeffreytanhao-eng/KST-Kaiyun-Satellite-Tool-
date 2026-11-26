import { onBeforeUnmount, ref, shallowRef, type Ref } from 'vue'
import * as Cesium from 'cesium'

import type { ScenarioData, SatelliteObject } from '@/api/scenarios'
import type {
  AccessReport,
  AccessWindow,
  AttitudeSample,
  AttitudeTrack,
  CoverageReport,
  LinkBudgetReport,
  LinkWindow,
  PropagationCompareReport,
  PropagationReport,
  RadarReport,
  SatelliteTrack,
  SensorSample,
  SensorTrack,
  TrackSample
} from '@/api/jobs'

const ACCESS_COLOR = '#ffb74d'

interface SatelliteVisual {
  data: SatelliteObject
  track: SatelliteTrack | null
  entity: Cesium.Entity
}

interface SensorVisual {
  track: SensorTrack
  satelliteEnabled: boolean
  entities: Cesium.Entity[]
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
  const sensorVisuals: SensorVisual[] = []

  let accessWindows: AccessWindow[] = []
  let accessSatelliteId: string | null = null
  let accessStationId: string | null = null
  let accessEntities: Cesium.Entity[] = []
  let linkEntity: Cesium.Entity | null = null
  let coveragePrimitive: Cesium.Primitive | null = null
  let playhead = 0
  let attitudeTracks = new Map<string, AttitudeTrack>()
  const attitudeEntities = new Map<string, Cesium.Entity[]>()
  let linkBudgetReport: LinkBudgetReport | null = null
  let linkTimeOffset = 0
  let linkBudgetEntities: Cesium.Entity[] = []
  let linkBudgetLine: Cesium.Entity | null = null
  let comparisonEntities: Cesium.Entity[] = []
  let radarReport: RadarReport | null = null
  let radarTimeOffset = 0
  let radarEntities: Cesium.Entity[] = []
  let radarLine: Cesium.Entity | null = null
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
    sensorVisuals.length = 0
    accessWindows = []
    accessSatelliteId = null
    accessStationId = null
    accessEntities = []
    linkEntity = null
    playhead = 0
    attitudeTracks = new Map()
    attitudeEntities.clear()
    linkBudgetReport = null
    linkBudgetEntities = []
    linkBudgetLine = null
    comparisonEntities = []
    radarReport = null
    radarEntities = []
    radarLine = null
  }

  function renderScenario(data: ScenarioData, report: PropagationReport | null) {
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
    for (const track of report?.satellites ?? []) {
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

    for (const track of report?.attitude_tracks ?? []) {
      if (!satellites.has(track.satellite_id) || track.samples.length === 0) continue
      attitudeTracks.set(track.satellite_id, track)
      const axes: Array<{ suffix: string; color: Cesium.Color }> = [
        { suffix: 'x', color: Cesium.Color.fromCssColorString('#ef5350') },
        { suffix: 'y', color: Cesium.Color.fromCssColorString('#66bb6a') },
        { suffix: 'z', color: Cesium.Color.fromCssColorString('#42a5f5') }
      ]
      attitudeEntities.set(
        track.satellite_id,
        axes.map((axis) =>
          instance.entities.add({
            id: `attitude-${track.satellite_id}-${axis.suffix}`,
            show: false,
            polyline: {
              positions: [] as Cesium.Cartesian3[],
              width: 3,
              arcType: Cesium.ArcType.NONE,
              material: new Cesium.PolylineGlowMaterialProperty({
                glowPower: 0.25,
                color: axis.color.withAlpha(0.95)
              })
            }
          })
        )
      )
    }

    const sensorMap = new Map(data.sensors.map((sensor) => [sensor.id, sensor]))
    for (const track of report?.sensor_tracks ?? []) {
      const sensor = sensorMap.get(track.sensor_id)
      const satellite = satellites.get(track.satellite_id)
      if (!sensor || !sensor.enabled || !satellite) continue
      const color = Cesium.Color.fromCssColorString(track.color || fallbackColor(0))
      const entities: Cesium.Entity[] = []

      if (track.shape === 'rectangular') {
        for (let index = 0; index < 4; index += 1) {
          entities.push(
            instance.entities.add({
              id: `sensor-edge-${track.sensor_id}-${index}`,
              show: false,
              polyline: {
                positions: [] as Cesium.Cartesian3[],
                width: 1.4,
                arcType: Cesium.ArcType.NONE,
                material: color.withAlpha(0.8)
              }
            })
          )
        }
        entities.push(
          instance.entities.add({
            id: `sensor-face-${track.sensor_id}`,
            show: false,
            polygon: {
              hierarchy: new Cesium.PolygonHierarchy([]),
              material: color.withAlpha(0.16),
              outline: true,
              outlineColor: color.withAlpha(0.55)
            }
          })
        )
      } else {
        const length = track.max_range_km * 1000
        entities.push(
          instance.entities.add({
            id: `sensor-cone-${track.sensor_id}`,
            show: false,
            cylinder: {
              length,
              topRadius: length * Math.tan(Cesium.Math.toRadians(track.half_angle_deg)),
              bottomRadius: 0,
              material: color.withAlpha(0.18),
              outline: true,
              outlineColor: color.withAlpha(0.5)
            }
          })
        )
      }

      sensorVisuals.push({
        track,
        satelliteEnabled: satellite.data.enabled,
        entities
      })
    }

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

    updateSensors(tSeconds)
    updateAttitude(tSeconds)
    updateAccessLink()
    updateLinkBudget()
    updateRadar()
  }

  /** 更新卫星本体坐标轴（长度约 120 km，按实时姿态方向）。 */
  function updateAttitude(tSeconds: number) {
    const axisLength = 120_000
    for (const [satelliteId, track] of attitudeTracks) {
      const entities = attitudeEntities.get(satelliteId)
      const visual = satellites.get(satelliteId)
      if (!entities || !visual?.track) continue
      const satelliteSample = sampleAt(visual.track.samples, tSeconds)
      const attitude = attitudeAt(track.samples, tSeconds)
      if (!satelliteSample || !attitude || !visual.data.enabled) {
        for (const entity of entities) entity.show = false
        continue
      }
      const origin = Cesium.Cartesian3.fromDegrees(
        satelliteSample.longitude_deg,
        satelliteSample.latitude_deg,
        satelliteSample.altitude_km * 1000
      )
      const axes = [attitude.x, attitude.y, attitude.z]
      entities.forEach((entity, index) => {
        const axis = axes[index]
        const tip = Cesium.Cartesian3.add(
          origin,
          Cesium.Cartesian3.multiplyByScalar(
            new Cesium.Cartesian3(axis[0], axis[1], axis[2]),
            axisLength,
            new Cesium.Cartesian3()
          ),
          new Cesium.Cartesian3()
        )
        if (entity.polyline) {
          entity.polyline.positions = new Cesium.ConstantProperty([origin, tip])
          entity.show = true
        }
      })
    }
  }

  function updateSensors(tSeconds: number) {
    for (const visual of sensorVisuals) {
      const satellite = satellites.get(visual.track.satellite_id)
      const satelliteSample = satellite?.track
        ? sampleAt(satellite.track.samples, tSeconds)
        : null
      const frame = sensorSampleAt(visual.track.samples, tSeconds)
      if (!visual.satelliteEnabled || !satelliteSample || !frame) {
        for (const entity of visual.entities) entity.show = false
        continue
      }

      const apex = Cesium.Cartesian3.fromDegrees(
        satelliteSample.longitude_deg,
        satelliteSample.latitude_deg,
        satelliteSample.altitude_km * 1000
      )
      const boresight = new Cesium.Cartesian3(...frame.boresight)
      const right = new Cesium.Cartesian3(...frame.right)
      const up = new Cesium.Cartesian3(...frame.up)
      const length = visual.track.max_range_km * 1000

      if (visual.track.shape === 'rectangular') {
        const halfHorizontal = Math.tan(
          Cesium.Math.toRadians(visual.track.horizontal_fov_deg / 2)
        ) * length
        const halfVertical = Math.tan(
          Cesium.Math.toRadians(visual.track.vertical_fov_deg / 2)
        ) * length
        const far = Cesium.Cartesian3.add(
          apex,
          Cesium.Cartesian3.multiplyByScalar(boresight, length, new Cesium.Cartesian3()),
          new Cesium.Cartesian3()
        )
        const corners = [
          [1, 1],
          [-1, 1],
          [-1, -1],
          [1, -1]
        ].map(([horizontal, vertical]) => {
          const offset = Cesium.Cartesian3.add(
            Cesium.Cartesian3.multiplyByScalar(
              right,
              horizontal * halfHorizontal,
              new Cesium.Cartesian3()
            ),
            Cesium.Cartesian3.multiplyByScalar(
              up,
              vertical * halfVertical,
              new Cesium.Cartesian3()
            ),
            new Cesium.Cartesian3()
          )
          return Cesium.Cartesian3.add(far, offset, new Cesium.Cartesian3())
        })

        for (let index = 0; index < 4; index += 1) {
          const edge = visual.entities[index]
          if (!edge.polyline) continue
          edge.polyline.positions = new Cesium.ConstantProperty([apex, corners[index]])
          edge.show = true
        }
        const face = visual.entities[4]
        if (face.polygon) {
          face.polygon.hierarchy = new Cesium.ConstantProperty(
            new Cesium.PolygonHierarchy(corners)
          )
          face.show = true
        }
      } else {
        const entity = visual.entities[0]
        const center = Cesium.Cartesian3.add(
          apex,
          Cesium.Cartesian3.multiplyByScalar(
            boresight,
            length / 2,
            new Cesium.Cartesian3()
          ),
          new Cesium.Cartesian3()
        )
        entity.position = new Cesium.ConstantPositionProperty(center)
        entity.orientation = new Cesium.ConstantProperty(
          quaternionFromFrame(frame)
        )
        entity.show = true
      }
    }
  }

  function showCoverage(report: CoverageReport | null) {
    const instance = viewer.value
    if (!instance) return
    clearCoverage()
    if (!report || report.points.length === 0) return

    const maxVisible = Math.max(report.max_visible_seconds, 1)
    const half = report.grid_step_deg / 2
    const instances: Cesium.GeometryInstance[] = []
    for (const point of report.points) {
      if (!point.covered) continue
      const ratio = Math.min(1, point.visible_seconds / maxVisible)
      const color = Cesium.Color.fromHsl(
        0.55 - 0.55 * ratio,
        0.85,
        0.5,
        0.3 + 0.35 * ratio
      )
      const south = Math.max(point.latitude_deg - half, -89.9)
      const north = Math.min(point.latitude_deg + half, 89.9)
      const west = Math.max(point.longitude_deg - half, -179.9)
      const east = Math.min(point.longitude_deg + half, 179.9)
      instances.push(
        new Cesium.GeometryInstance({
          geometry: new Cesium.RectangleGeometry({
            rectangle: Cesium.Rectangle.fromDegrees(west, south, east, north),
            height: 1500,
            vertexFormat: Cesium.PerInstanceColorAppearance.VERTEX_FORMAT
          }),
          attributes: {
            color: Cesium.ColorGeometryInstanceAttribute.fromColor(color)
          }
        })
      )
    }
    if (instances.length === 0) return

    coveragePrimitive = instance.scene.primitives.add(
      new Cesium.Primitive({
        geometryInstances: instances,
        appearance: new Cesium.PerInstanceColorAppearance({
          translucent: true,
          closed: false
        }),
        asynchronous: false
      })
    )
  }

  function clearCoverage() {
    const instance = viewer.value
    if (instance && coveragePrimitive) {
      instance.scene.primitives.remove(coveragePrimitive)
    }
    coveragePrimitive = null
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

  function marginColor(marginDb: number) {
    if (marginDb >= 3) return Cesium.Color.fromCssColorString('#66bb6a')
    if (marginDb >= 0) return Cesium.Color.fromCssColorString('#ffb74d')
    return Cesium.Color.fromCssColorString('#ef5350')
  }

  /** 显示链路预算：按余量着色的过境弧段 + 实时链路连线。 */
  function showLinkBudget(report: LinkBudgetReport | null, timeOffsetSeconds = 0) {
    const instance = viewer.value
    if (!instance) return
    clearLinkBudget()
    linkBudgetReport = report
    linkTimeOffset = timeOffsetSeconds
    if (!report) return

    const visual = satellites.get(report.satellite_id)
    if (!visual?.track) return
    const station = stations.get(report.ground_station_id)

    for (const window of report.windows) {
      const start = window.start_seconds + timeOffsetSeconds
      const end = window.end_seconds + timeOffsetSeconds
      const points = visual.track.samples.filter(
        (sample) => sample.t_seconds >= start && sample.t_seconds <= end
      )
      if (points.length < 2) continue
      const color = marginColor(window.min_margin_db)
      const width = window.min_margin_db >= 3 ? 3 : window.min_margin_db >= 0 ? 4.5 : 6
      linkBudgetEntities.push(
        instance.entities.add({
          id: `link-window-${window.index}`,
          polyline: {
            positions: Cesium.Cartesian3.fromDegreesArrayHeights(
              points.flatMap((sample) => [
                sample.longitude_deg,
                sample.latitude_deg,
                sample.altitude_km * 1000
              ])
            ),
            width,
            arcType: Cesium.ArcType.NONE,
            material: new Cesium.PolylineGlowMaterialProperty({
              glowPower: 0.28,
              color: color.withAlpha(0.9)
            })
          }
        })
      )
    }

    if (station) {
      linkBudgetLine = instance.entities.add({
        id: 'link-budget-live',
        show: false,
        polyline: {
          positions: [] as Cesium.Cartesian3[],
          width: 3,
          arcType: Cesium.ArcType.NONE,
          material: new Cesium.PolylineDashMaterialProperty({
            color: Cesium.Color.fromCssColorString('#66bb6a').withAlpha(0.95),
            dashLength: 10
          })
        }
      })
      updateLinkBudget()
    }
  }

  function updateLinkBudget() {
    if (!linkBudgetLine || !linkBudgetReport) return
    const visual = satellites.get(linkBudgetReport.satellite_id)
    const station = stations.get(linkBudgetReport.ground_station_id)
    if (!visual?.track || !station) {
      linkBudgetLine.show = false
      return
    }
    const shifted = linkBudgetReport.windows
      .map((window) => ({
        window,
        start: window.start_seconds + linkTimeOffset,
        end: window.end_seconds + linkTimeOffset
      }))
      .find((entry) => playhead >= entry.start && playhead <= entry.end)
    const satelliteSample = sampleAt(visual.track.samples, playhead)
    if (!shifted || !satelliteSample) {
      linkBudgetLine.show = false
      return
    }

    const localTime = playhead - linkTimeOffset
    const sample = linkSampleAt(shifted.window, localTime)
    const color = marginColor(sample?.margin_db ?? -99)
    const positions = [
      Cesium.Cartesian3.fromDegrees(
        station.longitude,
        station.latitude,
        station.altitudeKm * 1000
      ),
      Cesium.Cartesian3.fromDegrees(
        satelliteSample.longitude_deg,
        satelliteSample.latitude_deg,
        satelliteSample.altitude_km * 1000
      )
    ]
    if (linkBudgetLine.polyline) {
      linkBudgetLine.polyline.positions = new Cesium.ConstantProperty(positions)
      linkBudgetLine.polyline.material = new Cesium.PolylineDashMaterialProperty({
        color: color.withAlpha(0.95),
        dashLength: 10
      })
      linkBudgetLine.show = true
    }
  }

  function clearLinkBudget() {
    const instance = viewer.value
    if (!instance) return
    for (const entity of linkBudgetEntities) instance.entities.remove(entity)
    linkBudgetEntities = []
    if (linkBudgetLine) {
      instance.entities.remove(linkBudgetLine)
      linkBudgetLine = null
    }
    linkBudgetReport = null
  }

  /** 显示雷达探测范围与可探测弧段（绿色），并在窗口内绘制实时波束连线。 */
  function showRadar(report: RadarReport | null, timeOffsetSeconds = 0) {
    const instance = viewer.value
    if (!instance) return
    clearRadar()
    radarReport = report
    radarTimeOffset = timeOffsetSeconds
    if (!report) return

    const station = stations.get(report.ground_station_id)
    if (station) {
      const range = Math.max(report.max_detection_range_km * 1000, 1000)
      radarEntities.push(
        instance.entities.add({
          id: 'radar-coverage',
          position: Cesium.Cartesian3.fromDegrees(
            station.longitude,
            station.latitude,
            station.altitudeKm * 1000
          ),
          ellipsoid: {
            radii: new Cesium.Cartesian3(range, range, range),
            material: Cesium.Color.fromCssColorString('#66bb6a').withAlpha(0.12),
            outline: true,
            outlineColor: Cesium.Color.fromCssColorString('#66bb6a').withAlpha(0.55)
          }
        })
      )
    }

    const visual = satellites.get(report.satellite_id)
    if (visual?.track) {
      const color = Cesium.Color.fromCssColorString('#66bb6a')
      for (const window of report.windows) {
        if (window.max_snr_db < report.required_snr_db) continue
        const start = window.start_seconds + timeOffsetSeconds
        const end = window.end_seconds + timeOffsetSeconds
        const points = visual.track.samples.filter(
          (sample) => sample.t_seconds >= start && sample.t_seconds <= end
        )
        if (points.length < 2) continue
        radarEntities.push(
          instance.entities.add({
            id: `radar-arc-${window.index}`,
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
        )
      }
    }

    if (station) {
      radarLine = instance.entities.add({
        id: 'radar-live',
        show: false,
        polyline: {
          positions: [] as Cesium.Cartesian3[],
          width: 2,
          arcType: Cesium.ArcType.NONE,
          material: new Cesium.PolylineDashMaterialProperty({
            color: Cesium.Color.fromCssColorString('#66bb6a').withAlpha(0.9),
            dashLength: 10
          })
        }
      })
      updateRadar()
    }
  }

  function updateRadar() {
    if (!radarLine || !radarReport) return
    const visual = satellites.get(radarReport.satellite_id)
    const station = stations.get(radarReport.ground_station_id)
    if (!visual?.track || !station) {
      radarLine.show = false
      return
    }
    const shifted = radarReport.windows
      .map((window) => ({
        window,
        start: window.start_seconds + radarTimeOffset,
        end: window.end_seconds + radarTimeOffset
      }))
      .find((entry) => playhead >= entry.start && playhead <= entry.end)
    const satelliteSample = sampleAt(visual.track.samples, playhead)
    if (!shifted || !satelliteSample) {
      radarLine.show = false
      return
    }
    const localTime = playhead - radarTimeOffset
    const sample = nearestRadarSample(shifted.window.samples, localTime)
    const detected = Boolean(sample?.detected)
    if (radarLine.polyline) {
      radarLine.polyline.positions = new Cesium.ConstantProperty([
        Cesium.Cartesian3.fromDegrees(
          station.longitude,
          station.latitude,
          station.altitudeKm * 1000
        ),
        Cesium.Cartesian3.fromDegrees(
          satelliteSample.longitude_deg,
          satelliteSample.latitude_deg,
          satelliteSample.altitude_km * 1000
        )
      ])
      radarLine.polyline.material = new Cesium.PolylineDashMaterialProperty({
        color: Cesium.Color.fromCssColorString(detected ? '#66bb6a' : '#ef5350').withAlpha(0.95),
        dashLength: 10
      })
      radarLine.show = true
    }
  }

  function clearRadar() {
    const instance = viewer.value
    if (!instance) return
    for (const entity of radarEntities) instance.entities.remove(entity)
    radarEntities = []
    if (radarLine) {
      instance.entities.remove(radarLine)
      radarLine = null
    }
    radarReport = null
  }

  /** 显示 SGP4 与数值积分对比轨迹（实线 vs 虚线），并隐藏原始轨道线避免重叠。 */
  function showComparison(report: PropagationCompareReport | null) {
    const instance = viewer.value
    if (!instance) return
    clearComparison()
    if (!report) return

    const existing = instance.entities.getById(`orbit-${report.satellite_id}`)
    if (existing) existing.show = false

    const tracks: Array<{ track: SatelliteTrack; dashed: boolean }> = [
      { track: report.sgp4_track, dashed: false },
      { track: report.numerical_track, dashed: true }
    ]
    for (const { track, dashed } of tracks) {
      if (track.samples.length < 2) continue
      const color = Cesium.Color.fromCssColorString(track.color)
      comparisonEntities.push(
        instance.entities.add({
          id: `compare-${track.satellite_id}`,
          polyline: {
            positions: Cesium.Cartesian3.fromDegreesArrayHeights(
              track.samples.flatMap((sample) => [
                sample.longitude_deg,
                sample.latitude_deg,
                sample.altitude_km * 1000
              ])
            ),
            width: 3,
            arcType: Cesium.ArcType.NONE,
            material: dashed
              ? new Cesium.PolylineDashMaterialProperty({
                  color: color.withAlpha(0.95),
                  dashLength: 14
                })
              : new Cesium.PolylineGlowMaterialProperty({
                  glowPower: 0.2,
                  color: color.withAlpha(0.9)
                })
          }
        })
      )
    }
  }

  function clearComparison() {
    const instance = viewer.value
    if (!instance) return
    for (const entity of comparisonEntities) instance.entities.remove(entity)
    comparisonEntities = []
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
    showCoverage,
    clearCoverage,
    showLinkBudget,
    clearLinkBudget,
    showRadar,
    clearRadar,
    showComparison,
    clearComparison,
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

/** 插值传感器指向基，并重新归一化各轴。 */
function sensorSampleAt(
  samples: SensorSample[],
  tSeconds: number
): SensorSample | null {
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

  const lerp = (from: [number, number, number], to: [number, number, number]) =>
    normalizeVector([
      from[0] + (to[0] - from[0]) * fraction,
      from[1] + (to[1] - from[1]) * fraction,
      from[2] + (to[2] - from[2]) * fraction
    ])

  return {
    t_seconds: tSeconds,
    boresight: lerp(a.boresight, b.boresight),
    right: lerp(a.right, b.right),
    up: lerp(a.up, b.up)
  }
}

/** 插值卫星姿态采样，并重新归一化三轴。 */
function attitudeAt(samples: AttitudeSample[], tSeconds: number): AttitudeSample | null {
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
  const lerp = (from: [number, number, number], to: [number, number, number]) =>
    normalizeVector([
      from[0] + (to[0] - from[0]) * fraction,
      from[1] + (to[1] - from[1]) * fraction,
      from[2] + (to[2] - from[2]) * fraction
    ])

  return {
    t_seconds: tSeconds,
    x: lerp(a.x, b.x),
    y: lerp(a.y, b.y),
    z: lerp(a.z, b.z),
    quaternion: a.quaternion
  }
}

/** 在 Access 窗口内按时间线性插值链路余量。 */
function linkSampleAt(window: LinkWindow, tSeconds: number) {
  if (window.samples.length === 0) return null
  const first = window.samples[0]
  const last = window.samples[window.samples.length - 1]
  if (tSeconds <= first.t_seconds) return first
  if (tSeconds >= last.t_seconds) return last
  const index = window.samples.findIndex(
    (sample, position) =>
      position < window.samples.length - 1 &&
      tSeconds >= sample.t_seconds &&
      tSeconds <= window.samples[position + 1].t_seconds
  )
  if (index < 0) return last
  const a = window.samples[index]
  const b = window.samples[index + 1]
  const span = b.t_seconds - a.t_seconds || 1
  const fraction = (tSeconds - a.t_seconds) / span
  return {
    ...a,
    t_seconds: tSeconds,
    margin_db: a.margin_db + (b.margin_db - a.margin_db) * fraction,
    elevation_deg: a.elevation_deg + (b.elevation_deg - a.elevation_deg) * fraction
  }
}

/** 在雷达采样序列中查找时间最接近的样本。 */
function nearestRadarSample(samples: RadarReport['samples'], tSeconds: number) {
  if (samples.length === 0) return null
  let nearest = samples[0]
  let best = Math.abs(nearest.t_seconds - tSeconds)
  for (const sample of samples) {
    const distance = Math.abs(sample.t_seconds - tSeconds)
    if (distance < best) {
      best = distance
      nearest = sample
    }
  }
  return nearest
}

function normalizeVector(
  vector: [number, number, number]
): [number, number, number] {
  const norm = Math.hypot(vector[0], vector[1], vector[2]) || 1
  return [vector[0] / norm, vector[1] / norm, vector[2] / norm]
}

/** 由右/上/前向轴构造旋转四元数（局部 +Z 对齐传感器视轴）。 */
function quaternionFromFrame(frame: SensorSample): Cesium.Quaternion {
  const [rx, ry, rz] = frame.right
  const [ux, uy, uz] = frame.up
  const [bx, by, bz] = frame.boresight
  const matrix = new Cesium.Matrix3(rx, ux, bx, ry, uy, by, rz, uz, bz)
  return Cesium.Quaternion.fromRotationMatrix(matrix)
}
