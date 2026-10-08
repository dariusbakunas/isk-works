import { describe, expect, test } from "vitest";

import type { Insight } from "../../../../api/finance-analytics";
import { describeInsight } from "../insight-text";

const base: Insight = {
  kind: "categoryChange",
  side: "spending",
  subject: "Mutaplasmids",
  amount: "156000000.0000",
  previous: "529000000.0000",
  total: "3290000000.0000",
  sharePercent: 4.7,
  changePercent: -70.5,
};

describe("describeInsight", () => {
  test("a category change reads from its numbers, direction and side", () => {
    const text = describeInsight(base);
    expect(text.title).toBe("Mutaplasmids spending down 71%");
    expect(text.body).toBe("156M vs 529M in the previous period.");
    expect(text.icon).toBe("down");
    // Spending falling is good news.
    expect(text.tone).toBe("good");
  });

  test("rising spend is bad, rising income is good", () => {
    expect(describeInsight({ ...base, changePercent: 40 }).tone).toBe("bad");
    const income = describeInsight({ ...base, side: "income", subject: "Ships", changePercent: 120 });
    expect(income.title).toBe("Ships income up 120%");
    expect(income.icon).toBe("up");
    expect(income.tone).toBe("good");
    expect(describeInsight({ ...base, side: "income", changePercent: -30 }).tone).toBe("bad");
  });

  test("location concentration names the share and the amounts", () => {
    const text = describeInsight({
      kind: "locationConcentration",
      side: "spending",
      subject: "Jita IV - Moon 4",
      amount: "1840000000.0000",
      previous: null,
      total: "3290000000.0000",
      sharePercent: 55.93,
      changePercent: null,
    });
    expect(text.title).toBe("56% of spending at Jita IV - Moon 4");
    expect(text.body).toBe("1.84B of 3.29B total spending.");
    expect(text.icon).toBe("place");
    expect(text.tone).toBe("warn");
  });

  test("character concentration names the share of income", () => {
    const text = describeInsight({
      kind: "characterConcentration",
      side: "income",
      subject: "Corvin",
      amount: "2310000000.0000",
      previous: null,
      total: "4818000000.0000",
      sharePercent: 47.95,
      changePercent: null,
    });
    expect(text.title).toBe("Corvin earned 48% of income");
    expect(text.body).toBe("2.31B of 4.82B total income.");
    expect(text.icon).toBe("person");
  });

  test("never claims a change it cannot compute", () => {
    const text = describeInsight({ ...base, changePercent: null, previous: null });
    expect(text.title).toBe("Mutaplasmids spending changed");
    expect(text.body).toBe("156M this period.");
  });
});

describe("fee burden", () => {
  const fee: Insight = {
    kind: "feeBurden",
    side: "spending",
    subject: "Taxes & fees",
    amount: "312000000.0000",
    previous: "300000000.0000",
    total: "4818000000.0000",
    sharePercent: 6.476,
    changePercent: 4,
  };

  test("says what fees cost and what share of income that is", () => {
    const text = describeInsight(fee);
    expect(text.title).toBe("Taxes & fees cost 312M");
    expect(text.body).toBe("6% of 4.82B income, up 4% on the previous period.");
    expect(text.icon).toBe("fee");
    expect(text.tone).toBe("warn");
  });

  test("leaves the comparison out when there is none", () => {
    expect(describeInsight({ ...fee, changePercent: null, previous: null }).body).toBe("6% of 4.82B income.");
    expect(describeInsight({ ...fee, changePercent: -10 }).body).toBe("6% of 4.82B income, down 10% on the previous period.");
  });
});
