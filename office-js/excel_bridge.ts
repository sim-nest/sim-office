export type ExcelRangeRequest = {
  worksheet: string;
  address: string;
};

export type ExcelRangeReply = {
  worksheet: string;
  address: string;
  values: unknown[][];
};

declare const Excel: any;

export async function readRange(req: ExcelRangeRequest): Promise<ExcelRangeReply> {
  return Excel.run(async (context: any) => {
    const sheet = context.workbook.worksheets.getItem(req.worksheet);
    const range = sheet.getRange(req.address);
    range.load("values,address");
    await context.sync();
    return { worksheet: req.worksheet, address: range.address, values: range.values };
  });
}
