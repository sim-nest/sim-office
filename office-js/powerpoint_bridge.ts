export type PowerPointInsertFormatting = "UseDestinationTheme" | "KeepSourceFormatting";

export type PowerPointInsertRequest = {
  pptxBase64: string;
  targetSlideId?: string;
  formatting: PowerPointInsertFormatting;
};

declare const PowerPoint: any;

export async function insertDeck(req: PowerPointInsertRequest): Promise<void> {
  if (!req?.pptxBase64 || req.pptxBase64.trim().length === 0) {
    throw new Error("pptxBase64 is required");
  }
  if (req.formatting !== "UseDestinationTheme" && req.formatting !== "KeepSourceFormatting") {
    throw new Error("formatting is invalid");
  }
  return PowerPoint.run(async (context: any) => {
    context.presentation.insertSlidesFromBase64(req.pptxBase64, {
      targetSlideId: req.targetSlideId,
      formatting: req.formatting,
    });
    await context.sync();
  });
}
