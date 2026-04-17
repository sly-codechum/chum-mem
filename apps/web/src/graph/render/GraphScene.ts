import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';

export interface GraphSceneConfig {
  container: HTMLElement;
  sidebarWidth: number;
  background: number;
  fogDensity: number;
}

/**
 * Manages the Three.js scene, camera, renderer, lights, controls, and resize.
 */
export class GraphScene {
  readonly scene: THREE.Scene;
  readonly camera: THREE.PerspectiveCamera;
  readonly renderer: THREE.WebGLRenderer;
  readonly controls: OrbitControls;

  private sidebarWidth: number;

  constructor(config: GraphSceneConfig) {
    this.sidebarWidth = config.sidebarWidth;

    // Scene
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(config.background);
    this.scene.fog = new THREE.FogExp2(config.background, config.fogDensity);

    // Camera
    const w = this.graphWidth();
    const h = window.innerHeight;
    this.camera = new THREE.PerspectiveCamera(60, w / h, 1, 10000);
    this.camera.position.set(0, 200, 800);

    // Renderer
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.setSize(w, h);
    config.container.appendChild(this.renderer.domElement);

    // Controls
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.05;
    this.controls.rotateSpeed = 0.5;
    this.controls.zoomSpeed = 0.8;
    this.controls.minDistance = 50;
    this.controls.maxDistance = 4000;
    this.controls.autoRotate = false;

    // Lighting
    this.scene.add(new THREE.AmbientLight(0xffffff, 1.8));

    const light1 = new THREE.PointLight(0x39d98a, 2, 3000);
    light1.position.set(300, 300, 500);
    this.scene.add(light1);

    const light2 = new THREE.PointLight(0x58a6ff, 1.5, 3000);
    light2.position.set(-300, -200, 400);
    this.scene.add(light2);

    const light3 = new THREE.PointLight(0xf0883e, 1, 2500);
    light3.position.set(0, 400, -300);
    this.scene.add(light3);

    // Resize
    window.addEventListener('resize', this.onResize);
  }

  graphWidth(): number {
    return window.innerWidth - this.sidebarWidth;
  }

  private onResize = (): void => {
    const w = this.graphWidth();
    const h = window.innerHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
  };

  render(): void {
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }

  getCameraDistance(): number {
    return this.camera.position.length();
  }

  /**
   * Smoothly animate the camera to look at (and orbit around) a target position,
   * placing the camera at the given distance from the target.
   */
  focusOnPosition(target: THREE.Vector3, distance = 200): void {
    const cam = this.camera;
    const ctrl = this.controls;

    // Direction from target to current camera
    const dir = cam.position.clone().sub(ctrl.target).normalize();

    const endTarget = target.clone();
    const endPos = target.clone().add(dir.multiplyScalar(distance));

    // Animate over ~40 frames (~0.66s at 60fps)
    const startTarget = ctrl.target.clone();
    const startPos = cam.position.clone();
    let frame = 0;
    const totalFrames = 40;

    const step = () => {
      frame++;
      const t = frame / totalFrames;
      // ease-out cubic
      const e = 1 - Math.pow(1 - t, 3);

      cam.position.lerpVectors(startPos, endPos, e);
      ctrl.target.lerpVectors(startTarget, endTarget, e);
      ctrl.update();

      if (frame < totalFrames) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }

  dispose(): void {
    window.removeEventListener('resize', this.onResize);
    this.controls.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}
