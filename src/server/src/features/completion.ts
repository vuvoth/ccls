import { readFileSync } from 'fs';
import path = require('path');
import { CompletionItem, CompletionItemKind, TextDocumentPositionParams, TextEdit } from 'vscode-languageserver';
import { Position, TextDocument } from 'vscode-languageserver-textdocument';
import { URI } from 'vscode-uri';
import { LspDatabase } from '../db';


const KEYWORDS = [
	"var",
	"signal",
	"private",
	"input",
	"output",
	"linearCombination",
	"component",
	"template",
	"function",
	"if",
	"else",
	"for",
	"while",
	"compute",
	"do",
	"return",
	"include",
	'==>',
	'<==',
	'-->',
	'<--',
	'===',
	'>>=',
	'<<=',
];


export const KEYWORD_ITEMS: CompletionItem[] = KEYWORDS.map(keyword  => ({
	label: keyword, 
	kind: CompletionItemKind.Keyword,
	data: CompletionItemKind.Keyword
}));

function getDeclaredItems(tree: any): CompletionItem[] {
	if (tree === undefined) return [];

	let nodeInfos: CompletionItem[] = [];

	if (tree.name != undefined && (tree.type === "TEMPLATEDEF" || tree.type === "DECLARE")) {
		const name = (typeof tree.name === "string") ? tree.name : tree.name.name;
		const kind = (tree.type === "TEMPLATEDEF") ? CompletionItemKind.Function : CompletionItemKind.Variable;
		nodeInfos.push({
			label: name,
			kind,
			data: kind
		});
	}

	switch (tree.type) {
		case "BLOCK": {
			for (const node of tree.statements) {
				nodeInfos = nodeInfos.concat(getDeclaredItems(node));
			}
			break;
		}
		case "TEMPLATEDEF": {
			for (const node of tree.block.statements) {
				nodeInfos = nodeInfos.concat(getDeclaredItems(node));
			}
			break;
		}
		case "OP": {
			for (const node of tree.values) {
				nodeInfos = nodeInfos.concat(getDeclaredItems(node));
			}
		}
	}

	return nodeInfos;
}

// eslint-disable-next-line @typescript-eslint/no-empty-function
export function getAllCompletionItems(db: LspDatabase, document: TextDocument, position: Position): CompletionItem[] {
	const tree = db.getLatestParseTree(document.uri);
	const declareItems = getDeclaredItems(tree);
	
	const circomImportFiles = [];

	const filePath = URI.parse(document.uri).fsPath;

	if (tree.type === "BLOCK") {
		for (const node of tree.statements) {
			if (node.type === "INCLUDE") {
				const libFile = path.join(path.dirname(filePath), node.file);
				const content = readFileSync(libFile, {encoding: 'utf8'});
				circomImportFiles.push({content, uri: URI.file(libFile)});
			}
		}	
	}

	circomImportFiles.forEach((libFile) => {
		const syntax_tree = db.getLatestParseTree(libFile.uri.toString());

		declareItems.push(...getDeclaredItems(syntax_tree));
	});

	let offSet = document.offsetAt(position);
	
	// go to the last char changed.
	if (offSet > 0) 
		offSet = offSet - 1; 

	// if latest char is trigger compelete we only show variables
	if (document.getText().at(offSet) === ".") {
		return declareItems;
	}

	return KEYWORD_ITEMS.concat(declareItems);
}