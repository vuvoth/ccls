import { fstat, readFileSync } from 'fs';
import path = require('path');
import * as vscode from 'vscode-languageserver';
import { Location, Range } from 'vscode-languageserver';
import {
	TextDocument
} from 'vscode-languageserver-textdocument';
import { URI } from 'vscode-uri';
import { LspDatabase } from '../db';

// eslint-disable-next-line @typescript-eslint/no-var-requires
const Parser = require('circom/parser/jaz.js');

function inside(node: any, position: any) {
	if (node.first_line === position.line + 1 && node.first_column > position.character) return false;
	if (node.last_line === position.line + 1 && node.last_column < position.character) return false;
	return (node.first_line <= position.line + 1 && node.last_line >= position.line + 1);
}


function findNode(tree: any, position: any): any {
	switch (tree.type) {
		case "BLOCK": {
			for (const node of tree.statements) {
				if (inside(node, position)) {
					return findNode(node, position);
				}
			}
			break;
		}
		case "TEMPLATEDEF": {
			for (const node of tree.block.statements) {
				if (inside(node, position)) {
					return findNode(node, position);
				}
			}
			break;
		}
		case "OP": {
			for (const node of tree.values) {
				if (inside(node, position)) {
					return findNode(node, position);
				}
			}
		}
	}

	if (inside(tree, position)) {
		return tree;
	}

	return undefined;
}


function findNodeDefinition(tree: any, def: any, con?: any): any {
	let definitions: any[] = [];

	if (tree.name === def.name && (tree.type === "DECLARE" || tree.type === "TEMPLATEDEF")) {
		return [tree];
	}

	if (tree.type === "DECLARE" && tree.name.name == def.name) {
		return [tree.name];
	}

	switch (tree.type) {
		case "BLOCK": {
			for (const node of tree.statements) {
				definitions = definitions.concat(findNodeDefinition(node, def));
			}
			break;
		}
		case "TEMPLATEDEF": {
			for (const node of tree.block.statements) {
				definitions = definitions.concat(findNodeDefinition(node, def));
			}
			break;
		}
		case "OP": {
			for (const node of tree.values) {
				definitions = definitions.concat(findNodeDefinition(node, def));
			}
		}
	}

	return definitions;
}

function findDefinition(db: LspDatabase, document: TextDocument, pos: vscode.Position, con?: any): Thenable<vscode.Location | vscode.Location[]> {
	const tree = db.getLatestParseTree(document.uri);
	
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

	const greenNode = findNode(tree, pos);
	const location: vscode.Location[] = findNodeDefinition(tree, greenNode, con).map(
		(nodePos: any) => {
			return Location.create(
				document.uri,
				Range.create(
					vscode.Position.create(
						nodePos.first_line - 1,
						nodePos.first_column,
					),
					vscode.Position.create(
						nodePos.last_line - 1,
						nodePos.last_column
					)
				)
			);
		}
	);

	circomImportFiles.forEach((files) => {
		const syntax_tree = Parser.parse(files.content);
		location.push(...findNodeDefinition(syntax_tree, greenNode).map(
			(nodePos: any) => {
				return Location.create(
					files.uri.toString(),
					Range.create(
						vscode.Position.create(
							nodePos.first_line - 1,
							nodePos.first_column,
						),
						vscode.Position.create(
							nodePos.last_line - 1,
							nodePos.last_column
						)
					)
				);
			}
		));
	});

	return Promise.resolve(location);
}

export {
	findDefinition
};