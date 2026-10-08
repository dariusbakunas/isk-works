export function formatAssetDecimal(value: string) {
  const [integer, fraction = ""] = value.split(".");
  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const significantFraction = fraction.replace(/0+$/, "");
  return significantFraction ? `${grouped}.${significantFraction}` : grouped;
}
