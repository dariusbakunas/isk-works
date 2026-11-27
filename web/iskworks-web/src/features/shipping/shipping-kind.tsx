import { Truck } from "lucide-react";

import {
  SHIPPING_KIND,
  emptyShippingState,
  mergeShipping,
  type ShippingState,
} from "../../components/shipping/shipping-state";
import type { ToolWindowKind } from "../../components/tool-windows/tool-windows-context";
import { ShippingWindow } from "./shipping-window";

/** The Shipping calculator tool window: a load split into hold-sized shipments. */
export const shippingKind: ToolWindowKind<ShippingState> = {
  kind: SHIPPING_KIND,
  icon: Truck,
  title: () => "Shipping calculator",
  defaultSize: { w: 520, h: 620 },
  minSize: { w: 400, h: 320 },
  merge: mergeShipping,
  launcher: {
    label: "Shipping calculator",
    keywords: ["shipping", "cargo", "haul", "hauling", "freight", "volume", "m3", "calculator"],
    initialState: emptyShippingState,
  },
  render: (props) => <ShippingWindow {...props} />,
};
