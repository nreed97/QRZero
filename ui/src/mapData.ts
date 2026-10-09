import { feature, mesh } from "topojson-client";
import type { GeometryCollection, Topology } from "topojson-specification";
import world from "world-atlas/countries-110m.json";

const topo = world as unknown as Topology<{ countries: GeometryCollection; land: GeometryCollection }>;
export const land = feature(topo, topo.objects.land);
export const borders = mesh(topo, topo.objects.countries, (a, b) => a !== b);
