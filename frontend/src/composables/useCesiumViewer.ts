import { onBeforeUnmount, ref, shallowRef, type Ref } from 'vue'
import * as Cesium from 'cesium'

import type { StateResult } from '@/api/compute'

const SATELLITE_LABEL = 'KST-SAT-01'
const ACCENT = '#4fc3f7'

/**
 * Cesium 视口与实体渲染：初始化、离线底图加载、卫星/轨道绘制与相机飞行。
 * 所有轨道数据均由后端计算，这里只做可视化。
 */
export function useCesiumViewer(host: Ref<HTMLDivElement | null>) {
  const viewer = shallowRef<Cesium.Viewer | null>(null)
  const ready = ref(false)

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

  function renderState(data: StateResult) {
    const instance = viewer.value
    if (!instance) return

    instance.entities.removeAll()

    const position = Cesium.Cartesian3.fromDegrees(
      data.longitude_deg,
      data.latitude_deg,
      data.altitude_km * 1000
    )

    if (data.orbit_track.length > 1) {
      const flat = data.orbit_track.flatMap(([longitude, latitude, altitude]) => [
        longitude,
        latitude,
        altitude * 1000
      ])
      instance.entities.add({
        polyline: {
          positions: Cesium.Cartesian3.fromDegreesArrayHeights(flat),
          width: 3,
          arcType: Cesium.ArcType.NONE,
          material: new Cesium.PolylineGlowMaterialProperty({
            glowPower: 0.18,
            taperPower: 1.0,
            color: Cesium.Color.fromCssColorString(ACCENT).withAlpha(0.9)
          })
        }
      })
    }

    // 光晕层
    instance.entities.add({
      position,
      point: {
        pixelSize: 26,
        color: Cesium.Color.fromCssColorString(ACCENT).withAlpha(0.18),
        disableDepthTestDistance: Number.POSITIVE_INFINITY
      }
    })

    // 卫星本体 + 标签
    instance.entities.add({
      position,
      point: {
        pixelSize: 10,
        color: Cesium.Color.WHITE,
        outlineColor: Cesium.Color.fromCssColorString(ACCENT),
        outlineWidth: 2,
        disableDepthTestDistance: Number.POSITIVE_INFINITY
      },
      label: {
        text: SATELLITE_LABEL,
        font: '13px "Segoe UI", sans-serif',
        fillColor: Cesium.Color.WHITE,
        showBackground: true,
        backgroundColor: Cesium.Color.fromCssColorString('rgba(6, 12, 22, 0.72)'),
        backgroundPadding: new Cesium.Cartesian2(8, 4),
        pixelOffset: new Cesium.Cartesian2(0, -26),
        disableDepthTestDistance: Number.POSITIVE_INFINITY
      }
    })

    instance.camera.flyTo({
      destination: Cesium.Cartesian3.fromDegrees(
        data.longitude_deg,
        Math.max(-80, Math.min(80, data.latitude_deg - 18)),
        17_000_000
      ),
      orientation: {
        heading: 0,
        pitch: Cesium.Math.toRadians(-72),
        roll: 0
      },
      duration: 1.4
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

  return { viewer, ready, init, renderState, destroy }
}
